use core::f32;
use std::{collections::VecDeque, marker::PhantomData, path::PathBuf, time::Duration};

use bevy::{
    app::Update,
    asset::{AssetServer, Handle, LoadState, RecursiveDependencyLoadState, UntypedAssetId},
    ecs::{
        component::Component,
        event::{Event, Events},
        system::{Commands, EntityCommand, EntityCommands, Query, SystemParam},
        world::Command,
    },
    hierarchy::DespawnRecursiveExt,
    math::Vec3,
    pbr::StandardMaterial,
    prelude::{
        despawn_with_children_recursive, BuildWorldChildren, Bundle, Entity, GlobalTransform,
        IntoSystemConfigs, Mesh, Plugin, Res, World,
    },
    render::view::{Layer, RenderLayers},
    scene::{InstanceId, SceneSpawner},
    tasks::{IoTaskPool, Task},
};
use ethers_core::types::H160;
use futures_lite::future;
use smallvec::SmallVec;

pub struct UtilsPlugin;

impl Plugin for UtilsPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Update, despawn_with.in_set(SceneSets::RestrictedActions));
    }
}

pub fn config_file() -> PathBuf {
    project_directories().config_dir().join("config.json")
}

// get results from a task
pub trait TaskExt {
    type Output;
    fn complete(&mut self) -> Option<Self::Output>;
}

impl<T> TaskExt for Task<T> {
    type Output = T;

    fn complete(&mut self) -> Option<Self::Output> {
        match self.is_finished() {
            true => {
                Some(future::block_on(future::poll_once(self)).expect("is_finished but !Some?"))
            }
            false => None,
        }
    }
}

// convert string -> Address
pub trait AsH160 {
    fn as_h160(&self) -> Option<H160>;
}

impl AsH160 for &str {
    fn as_h160(&self) -> Option<H160> {
        if self.starts_with("0x") {
            return (&self[2..]).as_h160();
        }

        let Ok(hex_bytes) = hex::decode(self.as_bytes()) else {
            return None;
        };
        if hex_bytes.len() != H160::len_bytes() {
            return None;
        }

        Some(H160::from_slice(hex_bytes.as_slice()))
    }
}

impl AsH160 for String {
    fn as_h160(&self) -> Option<H160> {
        self.as_str().as_h160()
    }
}

pub fn format_address(address: H160, name: Option<&str>) -> String {
    let str_address = format!("{:x}", address);
    let str_address = str_address
        .chars()
        .skip(str_address.len().saturating_sub(4))
        .collect::<String>();
    if let Some(name) = name {
        format!("{name}#{str_address}")
    } else {
        format!(
            "0x{}...{str_address}",
            str_address.chars().take(4).collect::<String>()
        )
    }
}

/// a struct for buffering a certain amount of history and providing a subscription mechanism for updates
#[derive(Debug)]
pub struct RingBuffer<T: Clone + std::fmt::Debug> {
    log_source: tokio::sync::broadcast::Sender<T>,
    _log_sink: tokio::sync::broadcast::Receiver<T>,
    log_back: VecDeque<T>,
    back_capacity: usize,
    missed: usize,
}

impl<T: Clone + std::fmt::Debug> RingBuffer<T> {
    pub fn new(back_capacity: usize, reader_capacity: usize) -> Self {
        let (log_source, _log_sink) = tokio::sync::broadcast::channel(reader_capacity);

        Self {
            log_source,
            _log_sink,
            log_back: Default::default(),
            back_capacity,
            missed: 0,
        }
    }

    pub fn send(&mut self, item: T) {
        let _ = self.log_source.send(item.clone());
        if self.log_back.len() == self.back_capacity {
            self.log_back.pop_front();
            self.missed += 1;
        }
        self.log_back.push_back(item);
    }

    pub fn read(&self) -> (usize, Vec<T>, RingBufferReceiver<T>) {
        (
            self.missed,
            self.log_back.iter().cloned().collect(),
            self.log_source.subscribe(),
        )
    }
}

pub type RingBufferReceiver<T> = tokio::sync::broadcast::Receiver<T>;

// TryPushChildren command helper - add children but don't crash if any entities are already deleted
// if parent is deleted, despawn live children
// else add all live children to the parent
pub struct TryPushChildren {
    parent: Entity,
    children: SmallVec<[Entity; 8]>,
}

impl Command for TryPushChildren {
    fn apply(self, world: &mut World) {
        let live_children: SmallVec<[Entity; 8]> = self
            .children
            .into_iter()
            .filter(|c| world.entities().contains(*c))
            .collect();

        if let Some(mut entity) = world.get_entity_mut(self.parent) {
            entity.push_children(&live_children);
        } else {
            for child in live_children {
                despawn_with_children_recursive(world, child);
            }
        }
    }
}

pub struct TryChildBuilder<'a> {
    commands: Commands<'a, 'a>,
    push_children: TryPushChildren,
}

impl TryChildBuilder<'_> {
    /// Spawns an entity with the given bundle and inserts it into the parent entity's [`Children`].
    /// Also adds [`Parent`] component to the created entity.
    pub fn spawn(&mut self, bundle: impl Bundle) -> EntityCommands {
        let e = self.commands.spawn(bundle);
        self.push_children.children.push(e.id());
        e
    }

    /// Spawns an [`Entity`] with no components and inserts it into the parent entity's [`Children`].
    /// Also adds [`Parent`] component to the created entity.
    pub fn spawn_empty(&mut self) -> EntityCommands {
        let e = self.commands.spawn_empty();
        self.push_children.children.push(e.id());
        e
    }

    /// Returns the parent entity of this [`ChildBuilder`].
    pub fn parent_entity(&self) -> Entity {
        self.push_children.parent
    }

    /// Adds a command to be executed, like [`Commands::add`].
    pub fn add_command<C: Command>(&mut self, command: C) -> &mut Self {
        self.commands.add(command);
        self
    }
}
pub trait TryPushChildrenEx {
    fn try_with_children(&mut self, spawn_children: impl FnOnce(&mut TryChildBuilder))
        -> &mut Self;
    fn try_push_children(&mut self, children: &[Entity]) -> &mut Self;
}

impl TryPushChildrenEx for EntityCommands<'_> {
    fn try_push_children(&mut self, children: &[Entity]) -> &mut Self {
        let parent = self.id();
        self.commands().add(TryPushChildren {
            children: SmallVec::from(children),
            parent,
        });
        self
    }

    fn try_with_children(
        &mut self,
        spawn_children: impl FnOnce(&mut TryChildBuilder),
    ) -> &mut Self {
        let parent = self.id();
        let mut builder = TryChildBuilder {
            commands: self.commands(),
            push_children: TryPushChildren {
                children: SmallVec::default(),
                parent,
            },
        };

        spawn_children(&mut builder);
        let children = builder.push_children;
        if children.children.contains(&parent) {
            panic!("Entity cannot be a child of itself.");
        }
        self.commands().add(children);
        self
    }
}

pub struct FireEvent<E: Event> {
    event: E,
}

impl<E: Event> Command for FireEvent<E> {
    fn apply(self, world: &mut World) {
        let mut events = world.resource_mut::<Events<E>>();
        events.send(self.event);
    }
}

pub trait FireEventEx {
    fn fire_event<E: Event>(&mut self, e: E) -> &mut Self;
}

impl FireEventEx for Commands<'_, '_> {
    fn fire_event<E: Event>(&mut self, event: E) -> &mut Self {
        self.add(FireEvent { event });
        self
    }
}

// add a console command. trait is here as we want to mock it when testing
pub trait DoAddConsoleCommand {
    fn add_console_command<T: Command, U>(
        &mut self,
        system: impl IntoSystemConfigs<U>,
    ) -> &mut Self;
}

// macro for assertions
// by default, enabled in debug builds and disabled in release builds
// can be enabled for release with `cargo run --release --features="dcl-assert"`
#[cfg(any(debug_assertions, feature = "dcl-assert"))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => ( assert!($($arg)*); )
}
#[cfg(not(any(debug_assertions, feature = "dcl-assert")))]
#[macro_export]
macro_rules! dcl_assert {
    ($($arg:tt)*) => {};
}

pub use dcl_assert;

use crate::sets::SceneSets;

// quaternion normalization
pub trait QuatNormalizeExt {
    fn normalize_or_identity(&self) -> Self;
}

impl QuatNormalizeExt for bevy::prelude::Quat {
    fn normalize_or_identity(&self) -> Self {
        let norm = self.normalize();
        if norm.is_finite() {
            norm
        } else {
            bevy::prelude::Quat::IDENTITY
        }
    }
}

#[derive(Component)]
pub struct DespawnWith(pub Entity);

fn despawn_with(mut commands: Commands, q: Query<(Entity, &DespawnWith)>) {
    for (ent, with) in q.iter() {
        if commands.get_entity(with.0).is_none() {
            commands.entity(ent).despawn_recursive();
        }
    }
}

pub fn project_directories() -> directories::ProjectDirs {
    directories::ProjectDirs::from("org", "decentraland", "BevyExplorer").unwrap()
}

// commands to modify components

pub struct ModifyComponent<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> {
    func: F,
    _p: PhantomData<fn() -> C>,
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> EntityCommand
    for ModifyComponent<C, F>
{
    fn apply(self, id: Entity, world: &mut World) {
        if let Some(mut c) = world.get_mut::<C>(id) {
            (self.func)(&mut *c)
        }
    }
}

impl<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static> ModifyComponent<C, F> {
    fn new(func: F) -> Self {
        Self {
            func,
            _p: PhantomData,
        }
    }
}

pub trait ModifyComponentExt {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self;
}

impl ModifyComponentExt for EntityCommands<'_> {
    fn modify_component<C: Component, F: FnOnce(&mut C) + Send + Sync + 'static>(
        &mut self,
        func: F,
    ) -> &mut Self {
        self.add(ModifyComponent::new(func))
    }
}

pub struct ModifyDefaultComponent<C: Component + Default, F: FnOnce(&mut C) + Send + Sync + 'static>
{
    func: F,
    _p: PhantomData<fn() -> C>,
}

impl<C: Component + Default, F: FnOnce(&mut C) + Send + Sync + 'static> EntityCommand
    for ModifyDefaultComponent<C, F>
{
    fn apply(self, id: Entity, world: &mut World) {
        if let Some(mut c) = world.get_mut::<C>(id) {
            (self.func)(&mut *c)
        } else if let Some(mut entity) = world.get_entity_mut(id) {
            let mut v = C::default();
            (self.func)(&mut v);
            entity.insert(v);
        }
    }
}

impl<C: Component + Default, F: FnOnce(&mut C) + Send + Sync + 'static>
    ModifyDefaultComponent<C, F>
{
    fn new(func: F) -> Self {
        Self {
            func,
            _p: PhantomData,
        }
    }
}

pub trait ModifyDefaultComponentExt {
    fn default_and_modify_component<
        C: Component + Default,
        F: FnOnce(&mut C) + Send + Sync + 'static,
    >(
        &mut self,
        func: F,
    ) -> &mut Self;
}

impl ModifyDefaultComponentExt for EntityCommands<'_> {
    fn default_and_modify_component<
        C: Component + Default,
        F: FnOnce(&mut C) + Send + Sync + 'static,
    >(
        &mut self,
        func: F,
    ) -> &mut Self {
        self.add(ModifyDefaultComponent::new(func))
    }
}

#[macro_export]
macro_rules! anim_last_system {
    () => {
        bevy::prelude::expire_completed_transitions
    };
}

#[derive(Component, Default)]
pub struct AudioReceiver {
    pub layers: RenderLayers,
}

#[derive(SystemParam)]
pub struct VolumePanning<'w, 's> {
    receivers: Query<'w, 's, (&'static GlobalTransform, &'static AudioReceiver)>,
}

impl VolumePanning<'_, '_> {
    pub fn volume_and_panning(&self, translation: Vec3, layers: Option<&RenderLayers>) -> (f32, f32) {
        let (mut left, mut right): (f32, f32) = (0.0, 0.0);

        for (transform, _) in self
            .receivers
            .iter()
            .filter(|(_, receiver)| layers.unwrap_or_default().intersects(&receiver.layers))
        {
            let sound_path = translation - transform.translation();
            let volume = (1. - sound_path.length() / 75.0).clamp(0., 1.).powi(2);
            let panning = if sound_path.length() > f32::EPSILON {
                let right_ear_angle = transform.right().angle_between(sound_path);
                (right_ear_angle.cos() + 1.) / 2.
            } else {
                0.5
            };

            left += volume * (1.0 - panning);
            right += volume * panning;
        }

        let volume = left + right;
        let panning = right / (left + right).max(f32::EPSILON);

        (volume, panning)
    }
}

pub fn camera_to_render_layers<'a>(
    base: u32,
    camera_layers: impl Iterator<Item = &'a u32>,
) -> RenderLayers {
    camera_layers.fold(RenderLayers::none(), |result, camera_layer| {
        if *camera_layer == 0 {
            result.with(0 as Layer)
        } else {
            result.with((camera_layer - 1 + base) as Layer)
        }
    })
}

// convenient non-pooled client use for infrequent requests
pub fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .use_native_tls()
        .user_agent("DCLExplorer/0.1")
        .build()
        .unwrap()
}

pub trait TaskCompat {
    fn spawn_compat<T>(
        &self,
        future: impl core::future::Future<Output = T> + Send + 'static,
    ) -> Task<T>
    where
        T: Send + 'static;
}

impl TaskCompat for IoTaskPool {
    fn spawn_compat<T>(
        &self,
        future: impl core::future::Future<Output = T> + Send + 'static,
    ) -> Task<T>
    where
        T: Send + 'static,
    {
        self.spawn(async_compat::Compat::new(future))
    }
}

#[allow(clippy::type_complexity)]
#[derive(SystemParam)]
pub struct SceneSpawnerPlus<'w, 's> {
    scene_spawner: Res<'w, SceneSpawner>,
    asset_server: Res<'w, AssetServer>,
    query: Query<
        'w,
        's,
        (
            Option<&'static Handle<Mesh>>,
            Option<&'static Handle<StandardMaterial>>,
        ),
    >,
}

impl std::ops::Deref for SceneSpawnerPlus<'_, '_> {
    type Target = SceneSpawner;

    fn deref(&self) -> &Self::Target {
        &self.scene_spawner
    }
}

impl SceneSpawnerPlus<'_, '_> {
    pub fn instance_is_really_ready(&self, instance: InstanceId) -> bool {
        if !self.scene_spawner.instance_is_ready(instance) {
            return false;
        }

        fn check_handle(asset_server: &AssetServer, id: impl Into<UntypedAssetId>) -> bool {
            let id = id.into();
            if asset_server.load_state(id) == LoadState::Loading {
                return false;
            }

            if asset_server.recursive_dependency_load_state(id)
                == RecursiveDependencyLoadState::Loading
            {
                return false;
            }

            true
        }

        for scene_ent in self.scene_spawner.iter_instance_entities(instance) {
            let (maybe_h_mat, maybe_h_mesh) = self.query.get(scene_ent).unwrap();
            if maybe_h_mat.is_some_and(|h| !check_handle(&self.asset_server, h)) {
                return false;
            }
            if maybe_h_mesh.is_some_and(|h| !check_handle(&self.asset_server, h)) {
                return false;
            }
        }

        true
    }
}
