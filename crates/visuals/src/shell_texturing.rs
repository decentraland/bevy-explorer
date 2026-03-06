use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    pbr::NotShadowCaster,
    prelude::*,
    render::{
        mesh::MeshTag,
        render_resource::{AsBindGroup, ShaderRef},
    },
};
use common::structs::ParcelGrassConfig;

const PARCEL_GRASS_MESH: Handle<Mesh> = weak_handle!("75b4bc5b-7523-4d7c-a42f-d2ddb93ac169");
const PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("18c8dd1e-081d-452a-9c00-327775a239ff");
const IN_SCENE_PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("a7b403bc-917b-424e-878a-9714243bd4ce");

const IN_SCENE_PARCEL_GRASS_LAYERS: u32 = 5;
const IN_SCENE_PARCEL_GRASS_DISPLACEMENT: f32 = 0.01;

#[derive(Component)]
#[require(Transform, Visibility, ParcelGrassLod = ParcelGrassLod::High)]
pub struct ParcelGrass {
    pub parcel: IVec2,
}

#[derive(Clone, Copy, Component)]
#[component(immutable)]
#[repr(u8)]
pub enum ParcelGrassLod {
    High = 1,
    Mid = 2,
    Low = 3,
    InScene = 4,
}

#[derive(Component)]
pub struct ParcelGrassShell;

#[derive(Clone, Copy, Component)]
struct NeedsParcelGrass;

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
            rebuild_parcel_grasses.after(parcel_grass_config_updated),
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
            ParcelGrassLod::Low => (
                3,
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
