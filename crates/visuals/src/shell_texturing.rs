use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    ecs::{component::HookContext, spawn::SpawnableList, world::DeferredWorld},
    pbr::{MaterialPipeline, MaterialPipelineKey, NotShadowCaster},
    platform::collections::HashMap,
    prelude::*,
    render::{
        mesh::MeshTag,
        mesh::MeshVertexBufferLayoutRef,
        primitives::Aabb,
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
        },
        view::RenderLayers,
    },
    transform::TransformSystem,
};
use common::{
    sets::SetupSets,
    structs::{CurrentRealm, ParcelGrassConfig, PrimaryUser, GROUND_RENDERLAYER},
};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

use crate::terrain::{
    ground_extent, ground_mesh, ground_origin, parcel_mesh, terrain_vertex_shader,
    TerrainExtension, TerrainFlatMaterial, TerrainParams, TerrainSet, TerrainSurface,
    TERRAIN_TEXTURE,
};

const PARCEL_GRASS_MESH: Handle<Mesh> = weak_handle!("75b4bc5b-7523-4d7c-a42f-d2ddb93ac169");
// sloped parcels, by lod: 1 m, 2 m and 4 m cells
const PARCEL_GRASS_GRID_MESHES: [Handle<Mesh>; 3] = [
    weak_handle!("f39d29df-2e82-4d6f-9e3e-f50b4afdd286"),
    weak_handle!("0167dc0e-c82a-4d70-adee-9cf284608fbf"),
    weak_handle!("a3eaded3-7e8b-4146-8166-2bb92b94b4a7"),
];
const PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("18c8dd1e-081d-452a-9c00-327775a239ff");

const GROUND_MESH: Handle<Mesh> = weak_handle!("e2002cd1-4a0b-4944-ad26-97d64d72e5f9");
const GROUND_MATERIAL: Handle<ShellTexture> = weak_handle!("a7b403bc-917b-424e-878a-9714243bd4ce");
const GROUND_MATERIAL_FLAT_COLOR: Handle<TerrainFlatMaterial> =
    weak_handle!("3e91f222-a374-4f7f-ba1a-4a239c9734ae");
const GROUND_LAYERS: u32 = 5;
const GROUND_DISPLACEMENT: f32 = 0.01;
// rings beyond the parcel grass area, out to 13 km, covering the old 1024-parcel ground plane
const GROUND_RINGS: u32 = 6;
// terrain rises at most 8 steps of 1.8 m plus ~4.4 m of noise
const GROUND_MIN_Y: f32 = -5.0;
const GROUND_MAX_Y: f32 = 25.0;

const LOW_LOD: usize = 4;
const MID_LOD: usize = 2;
const HIGH_LOD: usize = 1;

#[derive(Default, Resource, Deref, DerefMut)]
struct ParcelGrassMap(HashMap<IVec2, Entity>);

#[derive(Component)]
#[require(Transform, Visibility, ParcelGrassLod = ParcelGrassLod::High)]
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
        // TODO: make this depend of the render distance
        // mirrored by `lod_cell` in terrain_vertex.wgsl, which stitches the ground and grass
        match distance {
            ..8 => ParcelGrassLod::High,
            8..16 => ParcelGrassLod::Mid,
            16.. => ParcelGrassLod::Low,
        }
    }
}

#[derive(Component)]
pub struct ParcelGrassShell;

/// Whether the shells were built with the flat quad.
#[derive(Component, PartialEq, Eq, Clone, Copy)]
struct ParcelGrassFlat(bool);

/// Huge plane that covers a huge area
#[derive(Component)]
struct Ground;

#[derive(Clone, Asset, TypePath, AsBindGroup)]
pub struct ShellTexture {
    // one uniform buffer: webgpu allows 12 per stage, shared with the view and mesh bindings
    #[uniform(0)]
    subdivisions: u32,
    #[uniform(0)]
    layers: u32,
    #[uniform(0)]
    padding: Vec2,
    #[uniform(0)]
    root_color: LinearRgba,
    #[uniform(0)]
    tip_color: LinearRgba,
    #[texture(100, sample_type = "float", filterable = false, visibility(vertex))]
    terrain_steps: Handle<Image>,
    #[uniform(101)]
    terrain: TerrainParams,
}

impl Material for ShellTexture {
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Opaque
    }

    fn vertex_shader() -> ShaderRef {
        terrain_vertex_shader()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        terrain_vertex_shader()
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

    // The prepass pipeline defaults to no culling while the main pass culls back faces, so shell
    // undersides wrote prepass depth that the main pass never drew over (the clear colour showed
    // when looking up at shells, e.g. from inside a hill). Draw both faces in both passes, so the
    // inside of a hill shows; the shader dithers shells out in front of the player.
    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ShellTexturingPlugin;

impl Plugin for ShellTexturingPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "shell_texturing.wgsl");

        app.init_state::<ParcelGrassState>();

        app.init_resource::<ParcelGrassMap>();
        app.init_resource::<ParcelGrassConfig>();

        app.add_plugins(MaterialPlugin::<ShellTexture>::default());

        app.add_systems(
            Startup,
            (setup_grass_meshes, spawn_ground.in_set(SetupSets::Main)),
        );
        app.add_systems(OnEnter(ParcelGrassState::Off), swap_ground);
        app.add_systems(OnEnter(ParcelGrassState::On), swap_ground);
        app.add_systems(
            PostUpdate,
            (
                (state_change, update_parcel_grass_material)
                    .run_if(resource_changed::<ParcelGrassConfig>),
                parcel_grass_config_updated
                    .run_if(resource_changed::<ParcelGrassConfig>.and(shells_need_updating)),
                (update_terrain_params, refresh_parcel_grass_meshes).after(TerrainSet),
                follow_player.before(TransformSystem::TransformPropagate),
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
                .run_if(player_changed_parcels.or(resource_exists_and_changed::<CurrentRealm>)),
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

fn setup_grass_meshes(mut meshes: ResMut<Assets<Mesh>>) {
    meshes.insert(PARCEL_GRASS_MESH.id(), parcel_mesh(1));
    for (handle, lod) in PARCEL_GRASS_GRID_MESHES
        .iter()
        .zip([HIGH_LOD, MID_LOD, LOW_LOD])
    {
        meshes.insert(handle.id(), parcel_mesh(grid_segments(lod)));
    }
    // the ground under the parcel grass uses the grass's own lod, relative to the player's parcel
    let segments = |offset| {
        grid_segments(match ParcelGrassLod::from_distance(IVec2::ZERO, offset) {
            ParcelGrassLod::High | ParcelGrassLod::Off => HIGH_LOD,
            ParcelGrassLod::Mid => MID_LOD,
            ParcelGrassLod::Low => LOW_LOD,
        })
    };
    meshes.insert(GROUND_MESH.id(), ground_mesh(segments, GROUND_RINGS));
}

/// Cells per parcel side of sloped parcel grass (and the ground under it) at a shell lod.
fn grid_segments(lod: usize) -> u32 {
    16 / lod as u32
}

fn ground_aabb() -> Aabb {
    let extent = ground_extent(GROUND_RINGS);
    Aabb::from_min_max(
        Vec3::new(-extent, GROUND_MIN_Y, -extent),
        Vec3::new(extent, GROUND_MAX_Y, extent),
    )
}

fn parcel_grass_aabb() -> Aabb {
    Aabb::from_min_max(
        Vec3::new(-8., GROUND_MIN_Y, -8.),
        Vec3::new(8., GROUND_MAX_Y, 8.),
    )
}

fn spawn_ground(mut commands: Commands, mut materials: ResMut<Assets<TerrainFlatMaterial>>) {
    commands.spawn((
        Name::new("Ground"),
        // follows the player's parcel, see `follow_player`
        Transform::from_translation(Vec3::new(0., -0.05, 0.)),
        Visibility::Inherited,
        ground_aabb(),
        Ground,
    ));
    materials.insert(
        GROUND_MATERIAL_FLAT_COLOR.id(),
        TerrainFlatMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.3, 0.45, 0.2),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                depth_bias: -100.0,
                fog_enabled: false,
                ..Default::default()
            },
            extension: TerrainExtension::default(),
        },
    );
}

fn follow_player(
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    mut ground: Query<&mut Transform, With<Ground>>,
) {
    let (Ok(player), Ok(mut ground)) = (player.single(), ground.single_mut()) else {
        return;
    };
    let translation = player.translation();
    if !translation.is_finite() {
        return;
    }
    let origin = ground_origin(translation).with_y(ground.translation.y);
    if ground.translation != origin {
        ground.translation = origin;
    }
}

fn update_terrain_params(
    surface: Res<TerrainSurface>,
    mut shells: ResMut<Assets<ShellTexture>>,
    mut flat: ResMut<Assets<TerrainFlatMaterial>>,
) {
    let changes = surface.changes();
    if !changes.params && !changes.relayout && !changes.player {
        return;
    }
    // touching the materials also rebinds the step texture after a relayout
    let params = surface.params();
    for handle in [&PARCEL_GRASS_MATERIAL, &GROUND_MATERIAL] {
        if let Some(material) = shells.get_mut(handle) {
            material.terrain = params;
        }
    }
    if let Some(material) = flat.get_mut(&GROUND_MATERIAL_FLAT_COLOR) {
        material.extension.terrain = params;
    }
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
                    Mesh3d(GROUND_MESH.clone()),
                    MeshMaterial3d(GROUND_MATERIAL_FLAT_COLOR),
                    GROUND_RENDERLAYER.clone(),
                ))
                .despawn_related::<Children>();
        }
        ParcelGrassState::On => {
            commands
                .entity(*ground)
                .try_insert(Children::spawn(ParcelGrassShellSpawnList {
                    shells: GROUND_LAYERS,
                    lod: HIGH_LOD,
                    displacement: GROUND_DISPLACEMENT,
                    mesh: GROUND_MESH.clone(),
                    material: GROUND_MATERIAL.clone(),
                    extras: (GROUND_RENDERLAYER, ground_aabb()),
                }))
                .remove::<(Mesh3d, MeshMaterial3d<TerrainFlatMaterial>, RenderLayers)>();
        }
    }
}

fn update_parcel_grass_material(
    mut materials: ResMut<Assets<ShellTexture>>,
    parcel_grass_config: Res<ParcelGrassConfig>,
    surface: Res<TerrainSurface>,
) {
    debug!(
        target: "visuals::parcel_grass::update_material",
        "Updating parcel grass material due to change in ParcelGrassConfig."
    );
    materials.insert(
        PARCEL_GRASS_MATERIAL.id(),
        ShellTexture {
            subdivisions: parcel_grass_config.subdivisions,
            layers: parcel_grass_config.layers,
            padding: Vec2::default(),
            root_color: parcel_grass_config.root_color.into(),
            tip_color: parcel_grass_config.tip_color.into(),
            terrain_steps: TERRAIN_TEXTURE,
            terrain: surface.params(),
        },
    );
    materials.insert(
        GROUND_MATERIAL.id(),
        ShellTexture {
            subdivisions: parcel_grass_config.subdivisions,
            layers: GROUND_LAYERS,
            padding: Vec2::default(),
            root_color: parcel_grass_config.root_color.into(),
            tip_color: parcel_grass_config.tip_color.into(),
            terrain_steps: TERRAIN_TEXTURE,
            terrain: surface.params(),
        },
    );
}

fn shells_need_updating(
    parcel_grass_config: Res<ParcelGrassConfig>,
    mut layers: Local<u32>,
    mut displacement: Local<f32>,
) -> bool {
    let old_layers = std::mem::replace(&mut *layers, parcel_grass_config.layers);
    let old_displacement =
        std::mem::replace(&mut *displacement, parcel_grass_config.y_displacement);

    old_layers != parcel_grass_config.layers
        || old_displacement != parcel_grass_config.y_displacement
}

fn parcel_grass_config_updated(
    mut commands: Commands,
    parcel_grasses: Query<Entity, With<ParcelGrassLod>>,
) {
    debug!(
        target: "visuals::parcel_grass::config_updated",
        "Rebuilding shells due to change in ParcelGrassConfig."
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
    surface: Res<TerrainSurface>,
) {
    let entity = trigger.target();
    let Ok((parcel_grass, parcel_grass_lod)) = parcel_grasses.get(entity) else {
        unreachable!("Infallible query");
    };

    let (lod, layers, displacement, material) = match parcel_grass_lod {
        ParcelGrassLod::Off => {
            return;
        }
        ParcelGrassLod::Low => (
            LOW_LOD,
            parcel_grass_config.layers,
            parcel_grass_config.y_displacement,
            &PARCEL_GRASS_MATERIAL,
        ),
        ParcelGrassLod::Mid => (
            MID_LOD,
            parcel_grass_config.layers,
            parcel_grass_config.y_displacement,
            &PARCEL_GRASS_MATERIAL,
        ),
        ParcelGrassLod::High => (
            HIGH_LOD,
            parcel_grass_config.layers,
            parcel_grass_config.y_displacement,
            &PARCEL_GRASS_MATERIAL,
        ),
    };
    debug!(
        target: "visuals::parcel_grass::rebuild",
        "Rebuilding shells for {entity} with lod {lod}."
    );

    let flat = surface.is_flat(parcel_grass.parcel);
    let mesh = if flat {
        PARCEL_GRASS_MESH.clone()
    } else {
        PARCEL_GRASS_GRID_MESHES[lod.trailing_zeros() as usize].clone()
    };

    commands.entity(entity).try_insert((
        ParcelGrassFlat(flat),
        Children::spawn(ParcelGrassShellSpawnList {
            shells: layers,
            displacement,
            lod,
            mesh,
            material: material.clone(),
            extras: (parcel_grass_aabb(),),
        }),
    ));
}

/// Rebuild parcel grass whose terrain became sloped (or flat) with the quad or grid mesh.
fn refresh_parcel_grass_meshes(
    mut commands: Commands,
    surface: Res<TerrainSurface>,
    parcel_grasses: Query<(Entity, &ParcelGrass, &ParcelGrassFlat), With<ParcelGrassLod>>,
) {
    let Some((min, max)) = surface.changes().targets else {
        return;
    };
    for (entity, parcel_grass, flat) in &parcel_grasses {
        let parcel = parcel_grass.parcel;
        if parcel.cmplt(min - 1).any() || parcel.cmpgt(max + 1).any() {
            continue;
        }
        if surface.is_flat(parcel) != flat.0 {
            commands.entity(entity).remove::<ParcelGrassLod>();
        }
    }
}

fn parcel_grass_lod_replaced(trigger: Trigger<OnReplace, ParcelGrassLod>, mut commands: Commands) {
    let entity = trigger.target();
    commands.entity(entity).queue_handled(
        |mut entity: EntityWorldMut| {
            entity.despawn_related::<Children>();
        },
        // This might happen on despawn, and if it is the case, just leave it be
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
) {
    let player_location = vec3_to_parcel(player.translation());

    // TODO: make this depend of the render distance
    for i in -7i32..=7 {
        let j_range = 7 - i.abs();
        for j in -j_range..=j_range {
            let parcel = player_location + IVec2::new(i, j);
            if !parcel_grass_map.contains_key(&parcel) {
                debug!(
                    target: "visuals::parcel_grass::fill",
                    "Creating parcel grass on parcel {parcel}."
                );
                commands.spawn((
                    Name::new("Parcel Grass"),
                    ParcelGrass { parcel },
                    Transform::from_translation(Vec3::new(
                        16. * parcel.x as f32 + 8.,
                        -0.05,
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
) {
    let player_location = vec3_to_parcel(player.translation());

    for (parcel_grass, entity) in parcel_grass_map.iter() {
        // TODO: make this depend of the render distance
        if player_location.distance_squared(*parcel_grass) > 150 {
            debug!(
                target: "visuals::parcel_grass::drop_far",
                "Dropping parcel grass for {} for being too far.",
                *parcel_grass
            );
            commands.entity(*entity).despawn();
        }
    }
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
        for i in (0..self.shells).step_by(self.lod) {
            world.spawn((
                ParcelGrassShell,
                Mesh3d(self.mesh.clone()),
                MeshMaterial3d(self.material.clone()),
                Transform::from_translation(Vec3::new(0., self.displacement * i as f32, 0.)),
                MeshTag(i + ((self.lod as u32) << 16)),
                NotShadowCaster,
                self.extras.clone(),
                ChildOf(entity),
            ));
        }
    }

    fn size_hint(&self) -> usize {
        (0..self.shells).step_by(self.lod).len()
    }
}
