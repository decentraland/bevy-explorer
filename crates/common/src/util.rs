use std::{collections::VecDeque, marker::PhantomData};

use bevy::{
    app::Update,
    ecs::{
        component::Component,
        event::{Event, Events},
        system::{Command, Commands, EntityCommand, EntityCommands, Query},
    },
    hierarchy::DespawnRecursiveExt,
    prelude::{
        despawn_with_children_recursive, BuildWorldChildren, Entity, IntoSystemConfigs, Plugin,
        World,
    },
    tasks::Task,
};
use ethers_core::types::H160;
use futures_lite::future;
use smallvec::SmallVec;

pub struct UtilsPlugin;

impl Plugin for UtilsPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_systems(Update, despawn_with);
    }
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

pub trait TryPushChildrenEx {
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

impl<'a> ModifyComponentExt for EntityCommands<'a> {
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

impl<'a> ModifyDefaultComponentExt for EntityCommands<'a> {
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
