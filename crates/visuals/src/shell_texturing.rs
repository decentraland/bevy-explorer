use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    ecs::{component::HookContext, world::DeferredWorld},
    pbr::NotShadowCaster,
    platform::collections::HashMap,
    prelude::*,
    render::{
        mesh::MeshTag,
        render_resource::{AsBindGroup, ShaderRef},
    },
};
use common::structs::{CurrentRealm, ParcelGrassConfig, PrimaryUser};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

const PARCEL_GRASS_MESH: Handle<Mesh> = weak_handle!("75b4bc5b-7523-4d7c-a42f-d2ddb93ac169");
const PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("18c8dd1e-081d-452a-9c00-327775a239ff");
const IN_SCENE_PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("a7b403bc-917b-424e-878a-9714243bd4ce");

const IN_SCENE_PARCEL_GRASS_LAYERS: u32 = 5;
const IN_SCENE_PARCEL_GRASS_DISPLACEMENT: f32 = 0.01;

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
#[repr(u8)]
pub enum ParcelGrassLod {
    Off = 0,
    High = 1,
    Mid = 2,
    Low = 3,
    InScene = 4,
}

#[derive(Component)]
pub struct ParcelGrassShell;

#[derive(Clone, Copy, Component)]
struct NeedsParcelGrass;

#[derive(Clone, Copy, Component)]
struct ParcelGrassWaitingScenePointer;

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

        app.init_resource::<ParcelGrassMap>();
        app.init_resource::<ParcelGrassConfig>();

        app.add_plugins(MaterialPlugin::<ShellTexture>::default());

        app.add_systems(Startup, setup_parcel_grass_mesh);
        app.add_systems(
            Update,
            (update_parcel_grass_material, parcel_grass_config_updated)
                .run_if(resource_changed::<ParcelGrassConfig>),
        );
        app.add_systems(
            Update,
            (parcel_grass_waiting_scene_pointer, rebuild_parcel_grasses)
                .chain()
                .after(parcel_grass_config_updated),
        );
        app.add_systems(
            PreUpdate,
            (fill_parcel_grass, drop_far_parcel_grass, recalculate_lod)
                .run_if(player_changed_parcels.or(resource_exists_and_changed::<CurrentRealm>)),
        );
        app.add_observer(parcel_grass_lod_change);
    }
}

fn setup_parcel_grass_mesh(mut meshes: ResMut<Assets<Mesh>>) {
    meshes.insert(
        PARCEL_GRASS_MESH.id(),
        Plane3d::new(Vec3::Y, Vec2::splat(8.)).mesh().build(),
    );
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
        IN_SCENE_PARCEL_GRASS_MATERIAL.id(),
        ShellTexture {
            subdivisions: parcel_grass_config.subdivisions,
            layers: 5,
            padding: Vec2::default(),
            root_color: parcel_grass_config.root_color.into(),
            tip_color: parcel_grass_config.tip_color.into(),
        },
    );
}

fn parcel_grass_config_updated(
    mut commands: Commands,
    parcel_grass: Query<Entity, With<ParcelGrassLod>>,
) {
    debug!(
        target: "visuals::parcel_grass::config_updated",
        "Rebuilding shells due to change in ParcelGrassConfig."
    );
    commands.insert_batch(
        parcel_grass
            .iter()
            .map(|entity| (entity, NeedsParcelGrass))
            .collect::<Vec<_>>(),
    );
}

fn parcel_grass_lod_change(trigger: Trigger<OnInsert, ParcelGrassLod>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(NeedsParcelGrass);
}

fn rebuild_parcel_grasses(
    mut commands: Commands,
    parcel_grasses: Populated<(Entity, &ParcelGrass, &ParcelGrassLod), With<NeedsParcelGrass>>,
    parcel_grass_config: Res<ParcelGrassConfig>,
) {
    for (entity, parcel_grass, parcel_grass_lod) in parcel_grasses.into_inner() {
        commands.entity(entity).despawn_related::<Children>();

        let (lod, layers, displacement, material) = match parcel_grass_lod {
            ParcelGrassLod::Off => {
                commands.entity(entity).remove::<NeedsParcelGrass>();
                continue;
            }
            ParcelGrassLod::Low => (
                4,
                parcel_grass_config.layers,
                parcel_grass_config.y_displacement,
                &PARCEL_GRASS_MATERIAL,
            ),
            ParcelGrassLod::Mid => (
                2,
                parcel_grass_config.layers,
                parcel_grass_config.y_displacement,
                &PARCEL_GRASS_MATERIAL,
            ),
            ParcelGrassLod::High => (
                1,
                parcel_grass_config.layers,
                parcel_grass_config.y_displacement,
                &PARCEL_GRASS_MATERIAL,
            ),
            ParcelGrassLod::InScene => (
                1,
                IN_SCENE_PARCEL_GRASS_LAYERS,
                IN_SCENE_PARCEL_GRASS_DISPLACEMENT,
                &IN_SCENE_PARCEL_GRASS_MATERIAL,
            ),
        };
        debug!(
            target: "visuals::parcel_grass::rebuild",
            "Rebuilding shells for {entity} with lod {lod}."
        );

        commands
            .entity(entity)
            .with_children(|parent| {
                for i in (0..layers).step_by(lod) {
                    parent.spawn((
                        ParcelGrassShell,
                        Mesh3d(PARCEL_GRASS_MESH.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(Vec3::new(
                            16. * parcel_grass.parcel.x as f32 + 8.,
                            -0.05 + (displacement * i as f32),
                            -(16. * parcel_grass.parcel.y as f32) - 8.,
                        )),
                        MeshTag(i),
                        NotShadowCaster,
                    ));
                }
            })
            .remove::<NeedsParcelGrass>();
    }
}

fn parcel_grass_waiting_scene_pointer(
    mut commands: Commands,
    parcel_grasses: Populated<
        (Entity, &ParcelGrass, &ParcelGrassLod),
        With<ParcelGrassWaitingScenePointer>,
    >,
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    scene_pointers: Res<ScenePointers>,
) {
    let parcel = vec3_to_parcel(player.translation());

    for (entity, parcel_grass, parcel_grass_lod) in parcel_grasses.into_inner() {
        match scene_pointers.get(parcel_grass.parcel) {
            Some(PointerResult::Nothing) => {
                let distance = parcel.distance_squared(parcel_grass.parcel);
                // TODO: make this depend of the render distance
                let lod = match distance {
                    ..0 => ParcelGrassLod::InScene,
                    0..8 => ParcelGrassLod::High,
                    8..16 => ParcelGrassLod::Mid,
                    16.. => ParcelGrassLod::Low,
                };
                let mut entity_cmd = commands.entity(entity);
                if lod != *parcel_grass_lod {
                    entity_cmd.insert((NeedsParcelGrass, lod));
                }
                entity_cmd.remove::<ParcelGrassWaitingScenePointer>();
            }
            Some(PointerResult::Exists { .. }) => {
                commands
                    .entity(entity)
                    .insert((NeedsParcelGrass, ParcelGrassLod::InScene))
                    .remove::<ParcelGrassWaitingScenePointer>();
            }
            None => {}
        }
    }
}

fn player_changed_parcels(
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    mut last_player_parcel: Local<Option<IVec2>>,
) -> bool {
    let current_parcel = vec3_to_parcel(player.translation());
    let old_parcel = last_player_parcel.replace(current_parcel);
    Some(current_parcel) != old_parcel
}

fn fill_parcel_grass(
    mut commands: Commands,
    player: Single<&GlobalTransform, With<PrimaryUser>>,
    parcel_grass_map: Res<ParcelGrassMap>,
) {
    let parcel = vec3_to_parcel(player.translation());

    // TODO: make this depend of the render distance
    for i in -7i32..=7 {
        let j_range = 7 - i.abs();
        for j in -j_range..=j_range {
            let parcel = parcel + IVec2::new(i, j);
            if !parcel_grass_map.contains_key(&parcel) {
                debug!(
                    target: "visuals::parcel_grass::fill",
                    "Creating parcel grass on parcel {parcel}."
                );
                commands.spawn((
                    ParcelGrass { parcel },
                    ParcelGrassLod::Off,
                    ParcelGrassWaitingScenePointer,
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
    let parcel = vec3_to_parcel(player.translation());

    for (parcel_grass, entity) in parcel_grass_map.iter() {
        // TODO: make this depend of the render distance
        if parcel.distance_squared(*parcel_grass) > 150 {
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
    parcel_grasses: Populated<Entity, Without<ParcelGrassWaitingScenePointer>>,
) {
    commands.try_insert_batch(
        parcel_grasses
            .iter()
            .map(|entity| (entity, ParcelGrassWaitingScenePointer))
            .collect::<Vec<_>>(),
    );
}
