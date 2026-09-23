use crate::grass_look::GrassLook;
use crate::ground_mask::{self, GroundMask};
use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    ecs::{component::HookContext, spawn::SpawnableList, world::DeferredWorld},
    pbr::NotShadowCaster,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    platform::collections::HashMap,
    prelude::*,
    render::mesh::{MeshTag, MeshVertexBufferLayoutRef},
    render::render_resource::ShaderRef,
    render::render_resource::{
        AsBindGroup, DepthBiasState, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    render::view::RenderLayers,
};
use bevy_console::ConsoleCommand;
use common::{
    sets::SetupSets,
    structs::{CurrentRealm, LandscapeTerrain, ParcelGrassConfig, PrimaryUser, GROUND_RENDERLAYER},
};
use console::DoAddConsoleCommand;
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

pub(super) const GROUND_MESH: Handle<Mesh> = weak_handle!("e2002cd1-4a0b-4944-ad26-97d64d72e5f9");
pub(super) const GROUND_FLAT_MESH: Handle<Mesh> =
    weak_handle!("4177ce82-b4ac-438e-afc1-a1a194e6dc5a");
const GROUND_MATERIAL: Handle<ShellTexture> = weak_handle!("a7b403bc-917b-424e-878a-9714243bd4ce");
const GROUND_MATERIAL_FLAT_COLOR: Handle<ShellTexture> =
    weak_handle!("3e91f222-a374-4f7f-ba1a-4a239c9734ae");
// One continuous turf surface. Tuft cards supply the silhouette, not stacked shells.
const GROUND_LAYERS: u32 = 1;
const GROUND_DISPLACEMENT: f32 = 0.01;
// keep ground triangles at 256 units a side: the shell shader reads the interpolated
// world_position to place blades and sample shadows, and the quad spans 16384 units
const GROUND_SUBDIVISIONS: u32 = 63;

const LOW_LOD: usize = 4;
const MID_LOD: usize = 2;
const HIGH_LOD: usize = 1;

#[derive(Default, Resource, Deref, DerefMut)]
struct ParcelGrassMap(HashMap<IVec2, Entity>);

#[derive(Component)]
#[require(Transform, Visibility)]
#[component(on_insert = Self::on_insert, on_replace = Self::on_replace)]
pub struct ParcelGrass {
    pub parcel: IVec2,
}

impl ParcelGrass {
    fn on_insert(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let parcel = deferred_world.get::<ParcelGrass>(entity).unwrap().parcel;

        let mut parcel_grass_map = deferred_world.resource_mut::<ParcelGrassMap>();
        parcel_grass_map.insert(parcel, entity);
    }

    fn on_replace(mut deferred_world: DeferredWorld, hook_context: HookContext) {
        let entity = hook_context.entity;
        let parcel = deferred_world.get::<ParcelGrass>(entity).unwrap().parcel;

        let mut parcel_grass_map = deferred_world.resource_mut::<ParcelGrassMap>();
        let removed_entity = parcel_grass_map.remove(&parcel).unwrap();
        assert_eq!(entity, removed_entity);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Component)]
#[component(immutable)]
pub enum ParcelGrassLod {
    Off,
    High,
    Mid,
    Low,
}

impl ParcelGrassLod {
    fn from_distance(player_location: IVec2, parcel: IVec2) -> Self {
        let distance = player_location.distance_squared(parcel);
        match distance {
            ..4 => ParcelGrassLod::High,
            4..9 => ParcelGrassLod::Mid,
            9.. => ParcelGrassLod::Low,
        }
    }
}

#[derive(Component)]
pub struct ParcelGrassShell;

#[derive(Component)]
pub(super) struct Ground;

#[derive(Clone, Asset, TypePath, AsBindGroup)]
#[bind_group_data(ShellTextureKey)]
pub struct ShellTexture {
    ground_depth_bias: bool,
    #[uniform(0)]
    subdivisions: u32,
    #[uniform(1)]
    layers: u32,
    #[uniform(2)]
    padding: Vec2,
    #[uniform(3)]
    root_color: LinearRgba,
    #[uniform(4)]
    tip_color: LinearRgba,
    #[texture(5, sample_type = "u_int")]
    ground_mask: Handle<Image>,
    #[uniform(6)]
    ground_detail: Vec4,
    #[texture(8)]
    #[sampler(9)]
    meadow: Handle<Image>,
    #[storage(101, read_only)]
    ambient: Handle<bevy::render::storage::ShaderStorageBuffer>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShellTextureKey {
    ground_depth_bias: bool,
}

impl From<&ShellTexture> for ShellTextureKey {
    fn from(material: &ShellTexture) -> Self {
        Self {
            ground_depth_bias: material.ground_depth_bias,
        }
    }
}

impl Material for ShellTexture {
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("shell_texturing.wgsl").display()
            )
            .into(),
        )
    }

    fn prepass_fragment_shader() -> ShaderRef {
        Self::fragment_shader()
    }

    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data.ground_depth_bias {
            if let Some(depth) = descriptor.depth_stencil.as_mut() {
                depth.bias = DepthBiasState {
                    constant: -1,
                    slope_scale: -1.0,
                    clamp: 0.0,
                };
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct ShellTexturingPlugin;

impl Plugin for ShellTexturingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shell_texturing.wgsl");
        embedded_asset!(app, "grass_palette.wgsl");
        embedded_asset!(app, "landscape_lighting.wgsl");
        embedded_asset!(app, "unity_ambient.wgsl");

        app.init_state::<ParcelGrassState>();

        app.init_resource::<ParcelGrassMap>();
        app.init_resource::<ParcelGrassConfig>();
        app.init_resource::<GroundMask>();
        app.add_plugins(crate::ground_visibility::GroundVisibilityPlugin);
        app.add_plugins(crate::terrain_loading::TerrainLoadingPlugin);
        app.add_plugins(crate::terrain_mesh::TerrainMeshPlugin);
        app.add_plugins(crate::grass_blades::GrassBladesPlugin);
        app.add_plugins(crate::landscape_trees::LandscapeTreesPlugin);
        app.add_plugins(crate::landscape_rigid::LandscapeRigidPlugin);
        app.add_plugins(crate::landscape_props::LandscapePropsPlugin);
        app.add_plugins(crate::landscape_coast::LandscapeCoastPlugin);

        app.add_plugins(MaterialPlugin::<ShellTexture>::default());

        app.add_systems(
            Startup,
            (setup_grass_meshes, spawn_ground.in_set(SetupSets::Main)),
        );
        app.add_console_command::<GrassConsoleCommand, _>(grass_console_command);
        app.add_systems(OnEnter(ParcelGrassState::Off), swap_ground);
        app.add_systems(OnEnter(ParcelGrassState::On), swap_ground);
        app.add_systems(
            PostUpdate,
            (
                ground_mask::update.run_if(
                    resource_changed::<ScenePointers>.or(resource_changed::<LandscapeTerrain>),
                ),
                state_change.run_if(resource_changed::<ParcelGrassConfig>),
                update_parcel_grass_material
                    .after(ground_mask::update)
                    .run_if(
                        resource_changed::<ParcelGrassConfig>.or(resource_changed::<GroundMask>),
                    ),
                parcel_grass_config_updated.run_if(grass_geometry_changed),
            ),
        );
        app.add_systems(
            Update,
            parcel_grass_without_lod.run_if(in_state(ParcelGrassState::On)),
        );
        app.add_systems(
            PreUpdate,
            ((fill_parcel_grass, drop_far_parcel_grass), recalculate_lod)
                .chain()
                .run_if(
                    player_changed_parcels
                        .or(resource_exists_and_changed::<CurrentRealm>)
                        .or(resource_changed::<ScenePointers>)
                        .or(resource_changed::<LandscapeTerrain>),
                ),
        );
        app.add_observer(parcel_grass_lod_inserted);
        app.add_observer(parcel_grass_lod_replaced);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, States)]
enum ParcelGrassState {
    #[default]
    Waiting,
    Off,
    On,
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/grass")]
struct GrassConsoleCommand {
    layers: u32,
    subdivisions: u32,
    y_displacement: f32,
    root_color: Option<String>,
    tip_color: Option<String>,
}

fn grass_console_command(
    mut input: ConsoleCommand<GrassConsoleCommand>,
    mut config: ResMut<ParcelGrassConfig>,
) {
    if let Some(Ok(command)) = input.take() {
        let parse = |hex: &Option<String>, fallback: Color| -> Result<Color, String> {
            match hex {
                None => Ok(fallback),
                Some(h) => Srgba::hex(h)
                    .map(Color::from)
                    .map_err(|e| format!("bad hex color `{h}`: {e:?}")),
            }
        };
        let root = match parse(&command.root_color, config.root_color) {
            Ok(c) => c,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        let tip = match parse(&command.tip_color, config.tip_color) {
            Ok(c) => c,
            Err(e) => {
                input.reply_failed(e);
                return;
            }
        };
        if !command.y_displacement.is_finite() || command.y_displacement <= 0.0 {
            input.reply_failed("Grass displacement must be a finite positive number.");
            return;
        }
        config.layers = command.layers.min(128);
        config.subdivisions = command.subdivisions.clamp(1, 128);
        config.y_displacement = command.y_displacement;
        config.root_color = root;
        config.tip_color = tip;
        input.reply_ok(format!(
            "grass layers {}, subdivisions {}, displacement {}",
            config.layers, config.subdivisions, config.y_displacement
        ));
    }
}

fn setup_grass_meshes(mut meshes: ResMut<Assets<Mesh>>) {
    reset_ground_meshes(&mut meshes);
}

pub(super) fn reset_ground_meshes(meshes: &mut Assets<Mesh>) {
    meshes.insert(
        GROUND_FLAT_MESH.id(),
        Plane3d::new(Vec3::Y, Vec2::splat(8.)).mesh().build(),
    );
    meshes.insert(
        GROUND_MESH.id(),
        Plane3d::new(Vec3::Y, Vec2::splat(8.))
            .mesh()
            .subdivisions(GROUND_SUBDIVISIONS)
            .build(),
    );
}

fn spawn_ground(
    mut commands: Commands,
    mut materials: ResMut<Assets<ShellTexture>>,
    mut images: ResMut<Assets<Image>>,
    mask: Res<GroundMask>,
    look: Res<GrassLook>,
    config: Res<ParcelGrassConfig>,
) {
    images.insert(
        crate::meadow_texture::MEADOW.id(),
        crate::meadow_texture::image(),
    );
    commands.spawn((
        Name::new("Ground"),
        Transform::from_scale(Vec3::new(1024., 1., 1024.)),
        Visibility::Inherited,
        NotShadowCaster,
        Ground,
    ));
    materials.insert(
        GROUND_MATERIAL_FLAT_COLOR.id(),
        ShellTexture {
            ground_depth_bias: true,
            subdivisions: 1,
            layers: 1,
            padding: Vec2::ZERO,
            root_color: config.root_color.to_linear(),
            tip_color: config.root_color.to_linear(),
            ground_mask: mask.0.clone(),
            ground_detail: look.ground(),
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
            meadow: crate::meadow_texture::MEADOW,
        },
    );
}

fn state_change(mut commands: Commands, parcel_grass_config: Res<ParcelGrassConfig>) {
    if parcel_grass_config.layers == 0 {
        debug!(
            target: "visuals::parcel_grass::set_state",
            "ParcelGrass is off."
        );
        commands.set_state(ParcelGrassState::Off);
    } else {
        debug!(
            target: "visuals::parcel_grass::set_state",
            "ParcelGrass is on."
        );
        commands.set_state(ParcelGrassState::On);
    }
}

fn swap_ground(
    mut commands: Commands,
    ground: Single<Entity, With<Ground>>,
    parcel_grass_state: Res<State<ParcelGrassState>>,
) {
    match parcel_grass_state.get() {
        ParcelGrassState::Waiting => {}
        ParcelGrassState::Off => {
            commands
                .entity(*ground)
                .try_insert((
                    Mesh3d(GROUND_FLAT_MESH.clone()),
                    MeshMaterial3d(GROUND_MATERIAL_FLAT_COLOR),
                    GROUND_RENDERLAYER.clone(),
                ))
                .despawn_related::<Children>();
        }
        ParcelGrassState::On => {
            commands
                .entity(*ground)
                .despawn_related::<Children>()
                .try_insert(Children::spawn(ParcelGrassShellSpawnList {
                    shells: GROUND_LAYERS,
                    lod: HIGH_LOD,
                    displacement: GROUND_DISPLACEMENT,
                    mesh: GROUND_MESH.clone(),
                    material: GROUND_MATERIAL.clone(),
                    extras: (GROUND_RENDERLAYER,),
                }))
                .remove::<(Mesh3d, MeshMaterial3d<ShellTexture>, RenderLayers)>();
        }
    }
}

fn update_parcel_grass_material(
    mut materials: ResMut<Assets<ShellTexture>>,
    parcel_grass_config: Res<ParcelGrassConfig>,
    mask: Res<GroundMask>,
    look: Res<GrassLook>,
) {
    debug!(
        target: "visuals::parcel_grass::update_material",
        "Updating parcel grass material due to change in ParcelGrassConfig."
    );
    materials.insert(
        GROUND_MATERIAL_FLAT_COLOR.id(),
        ShellTexture {
            ground_depth_bias: true,
            subdivisions: 1,
            layers: 1,
            padding: Vec2::default(),
            root_color: parcel_grass_config.root_color.into(),
            tip_color: parcel_grass_config.root_color.into(),
            ground_mask: mask.0.clone(),
            ground_detail: look.ground(),
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
            meadow: crate::meadow_texture::MEADOW,
        },
    );
    materials.insert(
        GROUND_MATERIAL.id(),
        ShellTexture {
            ground_depth_bias: true,
            subdivisions: parcel_grass_config.subdivisions,
            layers: GROUND_LAYERS,
            padding: Vec2::default(),
            root_color: parcel_grass_config.root_color.into(),
            tip_color: parcel_grass_config.tip_color.into(),
            ground_mask: mask.0.clone(),
            ground_detail: look.ground(),
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
            meadow: crate::meadow_texture::MEADOW,
        },
    );
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn grass_geometry_changed(
    config: Res<ParcelGrassConfig>,
    terrain: Res<LandscapeTerrain>,
    look: Res<GrassLook>,
    mut previous: Local<Option<(u32, u32, u32, u64, u32)>>,
) -> bool {
    // A camera-driven terrain mesh refresh changes rendered_generation, but not
    // the height field. Do not destroy/rebuild every grass patch for that change.
    let key = (
        config.layers,
        config.subdivisions,
        config.y_displacement.to_bits(),
        terrain.generation,
        look.shading.w.to_bits(),
    );
    previous.replace(key) != Some(key)
}

fn parcel_grass_config_updated(
    mut commands: Commands,
    parcel_grasses: Query<Entity, With<ParcelGrassLod>>,
) {
    debug!(
        target: "visuals::parcel_grass::config_updated",
        "Rebuilding grass geometry after a detail or height-field change."
    );
    for parcel_grass in parcel_grasses {
        commands.entity(parcel_grass).remove::<ParcelGrassLod>();
    }
}

fn parcel_grass_lod_inserted(
    trigger: Trigger<OnInsert, ParcelGrassLod>,
    mut commands: Commands,
    parcel_grasses: Query<(&ParcelGrass, &ParcelGrassLod)>,
    parcel_grass_config: Res<ParcelGrassConfig>,
    terrain: Res<LandscapeTerrain>,
    look: Res<GrassLook>,
) {
    let entity = trigger.target();
    let Ok((parcel, parcel_grass_lod)) = parcel_grasses.get(entity) else {
        unreachable!("Infallible query");
    };
    if parcel_grass_config.layers == 0 {
        return;
    }
    let lod = match parcel_grass_lod {
        ParcelGrassLod::Off => return,
        ParcelGrassLod::Low => LOW_LOD,
        ParcelGrassLod::Mid => MID_LOD,
        ParcelGrassLod::High => HIGH_LOD,
    };
    let parcel = parcel.parcel;
    let config = *parcel_grass_config;
    let field = terrain.field.clone();
    let look = *look;
    commands.spawn((
        crate::grass_blades::PendingGrass(crate::spawn_cpu(move || {
            crate::grass_blades::build_patch(parcel, config, lod, look, field.as_deref())
        })),
        Transform::default(),
        Visibility::Inherited,
        ChildOf(entity),
    ));
}

fn parcel_grass_lod_replaced(trigger: Trigger<OnReplace, ParcelGrassLod>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).queue_handled(
        |mut entity: EntityWorldMut| {
            entity.despawn_related::<Children>();
        },
        bevy::ecs::error::ignore,
    );
}

fn parcel_grass_without_lod(
    mut commands: Commands,
    parcel_grasses: Populated<(Entity, &ParcelGrass), Without<ParcelGrassLod>>,
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    scene_pointers: Res<ScenePointers>,
) {
    let player_location = vec3_to_parcel(player.translation());
    debug!(
        target: "visuals::parcel_grass::parcel_grass_without_lod",
        "Calculating LOD for {} entities.",
        parcel_grasses.iter().len()
    );

    for (entity, parcel_grass) in parcel_grasses.into_inner() {
        match scene_pointers.get(parcel_grass.parcel) {
            Some(PointerResult::Nothing) => {
                let lod = ParcelGrassLod::from_distance(player_location, parcel_grass.parcel);
                commands.entity(entity).try_insert(lod);
            }
            Some(PointerResult::Exists { .. }) => {
                commands.entity(entity).try_insert(ParcelGrassLod::Off);
            }
            None => {}
        }
    }
}

fn player_changed_parcels(
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    mut last_player_parcel: Local<IVec2>,
) -> bool {
    let player_location = vec3_to_parcel(player.translation());
    let old_parcel = std::mem::replace(&mut *last_player_parcel, player_location);
    player_location != old_parcel
}

fn fill_parcel_grass(
    mut commands: Commands,
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    parcel_grass_map: Res<ParcelGrassMap>,
    terrain: Res<LandscapeTerrain>,
) {
    let player_location = vec3_to_parcel(player.translation());

    // The shader finishes fading at 64 m. An 80 m population radius includes
    // a parcel-boundary margin without building invisible distant patches.
    for i in -5i32..=5 {
        for j in -5i32..=5 {
            if i * i + j * j > 25 {
                continue;
            }
            let parcel = player_location + IVec2::new(i, j);
            if !parcel_grass_map.contains_key(&parcel) && parcel_inside_terrain(&terrain, parcel) {
                debug!(
                    target: "visuals::parcel_grass::fill",
                    "Creating parcel grass on parcel {parcel}."
                );
                commands.spawn((
                    Name::new("Parcel Grass"),
                    ParcelGrass { parcel },
                    Transform::from_translation(Vec3::new(
                        16. * parcel.x as f32 + 8.,
                        0.0,
                        -(16. * parcel.y as f32) - 8.,
                    )),
                ));
            }
        }
    }
}

fn drop_far_parcel_grass(
    mut commands: Commands,
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    parcel_grass_map: Res<ParcelGrassMap>,
    terrain: Res<LandscapeTerrain>,
) {
    let player_location = vec3_to_parcel(player.translation());

    for (parcel_grass, entity) in parcel_grass_map.iter() {
        if player_location.distance_squared(*parcel_grass) > 49
            || !parcel_inside_terrain(&terrain, *parcel_grass)
        {
            debug!(
                target: "visuals::parcel_grass::drop_far",
                "Dropping parcel grass for {} for being too far.",
                *parcel_grass
            );
            commands.entity(*entity).despawn();
        }
    }
}

fn parcel_inside_terrain(terrain: &LandscapeTerrain, parcel: IVec2) -> bool {
    terrain.field.as_ref().is_none_or(|field| {
        field.contains(parcel.x as f32 * 16.0 + 8.0, parcel.y as f32 * 16.0 + 8.0)
    })
}

fn recalculate_lod(
    mut commands: Commands,
    parcel_grasses: Query<(Entity, &ParcelGrass, &ParcelGrassLod)>,
    maybe_player: Option<Single<&GlobalTransform, With<PrimaryUser>>>,
    scene_pointers: Res<ScenePointers>,
) {
    let player_location = if let Some(player) = maybe_player {
        vec3_to_parcel(player.translation())
    } else {
        IVec2::MAX
    };
    debug!(
        target: "visuals::parcel_grass::recalculate_lod",
        "Recalculating LOD for {} entities.",
        parcel_grasses.iter().len()
    );

    for (entity, parcel_grass, parcel_grass_lod) in parcel_grasses {
        match scene_pointers.get(parcel_grass.parcel) {
            Some(PointerResult::Nothing) => {
                let lod = ParcelGrassLod::from_distance(player_location, parcel_grass.parcel);
                if lod != *parcel_grass_lod {
                    commands.entity(entity).try_insert(lod);
                }
            }
            Some(PointerResult::Exists { .. }) => {
                commands.entity(entity).try_insert(ParcelGrassLod::Off);
            }
            None => {
                commands.entity(entity).remove::<ParcelGrassLod>();
            }
        }
    }
}

struct ParcelGrassShellSpawnList<B: Bundle + Clone> {
    shells: u32,
    lod: usize,
    displacement: f32,
    mesh: Handle<Mesh>,
    material: Handle<ShellTexture>,
    extras: B,
}

impl<B: Bundle + Clone> SpawnableList<ChildOf> for ParcelGrassShellSpawnList<B> {
    fn spawn(self, world: &mut World, entity: Entity) {
        let ParcelGrassShellSpawnList {
            shells,
            lod,
            displacement,
            mesh,
            material,
            extras,
        } = self;
        for i in (0..shells).step_by(lod) {
            world.spawn((
                ParcelGrassShell,
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(Vec3::new(0., displacement * i as f32, 0.)),
                MeshTag(i + ((lod as u32) << 16)),
                NotShadowCaster,
                extras.clone(),
                ChildOf(entity),
            ));
        }
    }

    fn size_hint(&self) -> usize {
        (0..self.shells).step_by(self.lod).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_mesh_refreshes_and_palette_changes_do_not_rebuild_grass() {
        #[derive(Resource, Default)]
        struct Rebuilds(usize);
        let mut app = App::new();
        app.init_resource::<ParcelGrassConfig>()
            .init_resource::<GrassLook>()
            .init_resource::<LandscapeTerrain>()
            .init_resource::<Rebuilds>()
            .add_systems(
                Update,
                (|mut rebuilds: ResMut<Rebuilds>| rebuilds.0 += 1).run_if(grass_geometry_changed),
            );
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 1);
        app.world_mut()
            .resource_mut::<LandscapeTerrain>()
            .rendered_generation = Some(0);
        app.world_mut()
            .resource_mut::<ParcelGrassConfig>()
            .root_color = Color::WHITE;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 1);
        app.world_mut()
            .resource_mut::<LandscapeTerrain>()
            .generation += 1;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 2);
        app.world_mut().resource_mut::<ParcelGrassConfig>().layers = 8;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 3);
        app.world_mut()
            .resource_mut::<ParcelGrassConfig>()
            .y_displacement = 0.04;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 4);
        app.world_mut()
            .resource_mut::<ParcelGrassConfig>()
            .subdivisions = 24;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 5);
        app.world_mut().resource_mut::<GrassLook>().form.x = 0.1;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 5);
        app.world_mut().resource_mut::<GrassLook>().shading.w = 0.1;
        app.update();
        assert_eq!(app.world().resource::<Rebuilds>().0, 6);
    }

    #[test]
    fn lod_changes_and_off_remove_pending_blades_without_orphans() {
        let mut app = App::new();
        app.add_plugins(bevy::app::TaskPoolPlugin::default())
            .init_resource::<GrassLook>()
            .init_resource::<ParcelGrassMap>()
            .init_resource::<ParcelGrassConfig>()
            .init_resource::<LandscapeTerrain>()
            .add_observer(parcel_grass_lod_inserted)
            .add_observer(parcel_grass_lod_replaced);
        let parcel = app
            .world_mut()
            .spawn(ParcelGrass {
                parcel: IVec2::ZERO,
            })
            .id();
        assert!(app.world().get::<ParcelGrassLod>(parcel).is_none());
        assert!(app.world().get::<Children>(parcel).is_none());
        app.world_mut()
            .entity_mut(parcel)
            .insert(ParcelGrassLod::High);
        let first = app.world().get::<Children>(parcel).unwrap()[0];
        assert!(app
            .world()
            .get::<crate::grass_blades::PendingGrass>(first)
            .is_some());
        app.world_mut()
            .entity_mut(parcel)
            .insert(ParcelGrassLod::Low);
        assert!(app.world().get_entity(first).is_err());
        let second = app.world().get::<Children>(parcel).unwrap()[0];
        assert_ne!(first, second);
        app.world_mut()
            .entity_mut(parcel)
            .insert(ParcelGrassLod::Off);
        assert!(app.world().get_entity(second).is_err());
        assert!(app.world().get::<Children>(parcel).is_none());
        app.world_mut().resource_mut::<ParcelGrassConfig>().layers = 0;
        app.world_mut()
            .entity_mut(parcel)
            .insert(ParcelGrassLod::High);
        assert!(app.world().get::<Children>(parcel).is_none());
    }

    #[test]
    fn rebuilding_ground_does_not_leave_orphaned_shells() {
        let mut app = App::new();
        app.insert_resource(State::new(ParcelGrassState::On))
            .add_systems(Update, swap_ground);
        let ground = app.world_mut().spawn((Ground, Transform::default())).id();
        app.update();
        let first_shells = app.world().get::<Children>(ground).unwrap().to_vec();
        app.update();

        assert!(first_shells
            .iter()
            .all(|entity| app.world().get_entity(*entity).is_err()));
        let world = app.world_mut();
        let mut shells = world.query_filtered::<Option<&ChildOf>, With<ParcelGrassShell>>();
        assert_eq!(shells.iter(world).count(), GROUND_LAYERS as usize);
        assert!(shells
            .iter(world)
            .all(|parent| parent.is_some_and(|parent| parent.parent() == ground)));
    }
}
