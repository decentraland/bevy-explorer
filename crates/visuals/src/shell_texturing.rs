use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    ecs::{component::HookContext, spawn::SpawnableList, world::DeferredWorld},
    pbr::NotShadowCaster,
    platform::collections::HashMap,
    prelude::*,
    render::{
        mesh::MeshTag,
        render_resource::{AsBindGroup, ShaderRef},
        view::RenderLayers,
    },
};
use common::{
    sets::SetupSets,
    structs::{CurrentRealm, ParcelGrassConfig, PrimaryUser, GROUND_RENDERLAYER},
};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

const PARCEL_GRASS_MESH: Handle<Mesh> = weak_handle!("75b4bc5b-7523-4d7c-a42f-d2ddb93ac169");
const PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("18c8dd1e-081d-452a-9c00-327775a239ff");

const GROUND_MATERIAL: Handle<ShellTexture> = weak_handle!("a7b403bc-917b-424e-878a-9714243bd4ce");
const GROUND_MATERIAL_FLAT_COLOR: Handle<StandardMaterial> =
    weak_handle!("3e91f222-a374-4f7f-ba1a-4a239c9734ae");
const GROUND_LAYERS: u32 = 5;
const GROUND_DISPLACEMENT: f32 = 0.01;

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
        match distance {
            ..8 => ParcelGrassLod::High,
            8..16 => ParcelGrassLod::Mid,
            16.. => ParcelGrassLod::Low,
        }
    }
}

#[derive(Component)]
pub struct ParcelGrassShell;

/// Huge plane that covers a huge area
#[derive(Component)]
struct Ground;

#[derive(Clone, Asset, TypePath, AsBindGroup)]
pub struct ShellTexture {
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
            (
                setup_parcel_grass_mesh,
                spawn_ground.in_set(SetupSets::Main),
            ),
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

fn setup_parcel_grass_mesh(mut meshes: ResMut<Assets<Mesh>>) {
    meshes.insert(
        PARCEL_GRASS_MESH.id(),
        Plane3d::new(Vec3::Y, Vec2::splat(8.)).mesh().build(),
    );
}

fn spawn_ground(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        // Ground covers 1024 parcels
        Transform::from_scale(Vec3::new(1024., 1., 1024.))
            .with_translation(Vec3::new(0., -0.05, 0.)),
        Ground,
    ));
    materials.insert(
        GROUND_MATERIAL_FLAT_COLOR.id(),
        StandardMaterial {
            base_color: Color::srgb(0.3, 0.45, 0.2),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            depth_bias: -100.0,
            fog_enabled: false,
            ..Default::default()
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
                .insert((
                    Mesh3d(PARCEL_GRASS_MESH.clone()),
                    MeshMaterial3d(GROUND_MATERIAL_FLAT_COLOR),
                    GROUND_RENDERLAYER.clone(),
                ))
                .despawn_related::<Children>();
        }
        ParcelGrassState::On => {
            commands
                .entity(*ground)
                .insert(Children::spawn(ParcelGrassShellSpawnList {
                    shells: GROUND_LAYERS,
                    lod: HIGH_LOD,
                    displacement: GROUND_DISPLACEMENT,
                    material: GROUND_MATERIAL.clone(),
                    extras: (GROUND_RENDERLAYER,),
                }))
                .remove::<(Mesh3d, MeshMaterial3d<StandardMaterial>, RenderLayers)>();
        }
    }
}

fn update_parcel_grass_material(
    mut materials: ResMut<Assets<ShellTexture>>,
    parcel_grass_config: Res<ParcelGrassConfig>,
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
    parcel_grasses: Query<&ParcelGrassLod>,
    parcel_grass_config: Res<ParcelGrassConfig>,
) {
    let entity = trigger.target();
    let Ok(parcel_grass_lod) = parcel_grasses.get(entity) else {
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

    commands
        .entity(entity)
        .insert(Children::spawn(ParcelGrassShellSpawnList {
            shells: layers,
            displacement,
            lod,
            material: material.clone(),
            extras: (),
        }));
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
                commands.entity(entity).insert(lod);
            }
            Some(PointerResult::Exists { .. }) => {
                commands.entity(entity).insert(ParcelGrassLod::Off);
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
                    commands.entity(entity).insert(lod);
                }
            }
            Some(PointerResult::Exists { .. }) => {
                commands.entity(entity).insert(ParcelGrassLod::Off);
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
    material: Handle<ShellTexture>,
    extras: B,
}

impl<B: Bundle + Clone> SpawnableList<ChildOf> for ParcelGrassShellSpawnList<B> {
    fn spawn(self, world: &mut World, entity: Entity) {
        for i in (0..self.shells).step_by(self.lod) {
            world.spawn((
                ParcelGrassShell,
                Mesh3d(PARCEL_GRASS_MESH.clone()),
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
