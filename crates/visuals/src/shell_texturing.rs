use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle},
    color::palettes,
    pbr::NotShadowCaster,
    prelude::*,
    render::{
        mesh::MeshTag,
        render_resource::{AsBindGroup, ShaderRef},
    },
};

const PARCEL_GRASS_MESH: Handle<Mesh> = weak_handle!("75b4bc5b-7523-4d7c-a42f-d2ddb93ac169");
const PARCEL_GRASS_MATERIAL: Handle<ShellTexture> =
    weak_handle!("18c8dd1e-081d-452a-9c00-327775a239ff");

#[derive(Component)]
#[require(Transform, Visibility)]
pub struct ParcelGrass {
    pub parcel: IVec2,
}

#[derive(Component)]
pub struct ParcelGrassShell;

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
            update_parcel_grass_material.run_if(resource_changed::<ParcelGrassConfig>),
        );
        app.add_observer(new_parcel_grass);
    }
}

#[derive(Resource)]
pub struct ParcelGrassConfig {
    pub layers: u32,
    pub subdivisions: u32,
    pub y_displacement: f32,
    pub root_color: Color,
    pub tip_color: Color,
}

impl Default for ParcelGrassConfig {
    fn default() -> Self {
        Self {
            layers: 32,
            subdivisions: 32,
            y_displacement: 0.01,
            root_color: palettes::tailwind::LIME_800.into(),
            tip_color: palettes::tailwind::LIME_600.into(),
        }
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
}

fn new_parcel_grass(
    trigger: Trigger<OnAdd, ParcelGrass>,
    mut commands: Commands,
    shell_texturing_grasses: Query<&ParcelGrass>,
    shell_texturing_config: Res<ParcelGrassConfig>,
) {
    let entity = trigger.target();
    let Ok(shell_texturing_grass) = shell_texturing_grasses.get(entity) else {
        unreachable!("Infallible query");
    };

    commands.entity(entity).with_children(|parent| {
        for i in 0..shell_texturing_config.layers {
            parent.spawn((
                ParcelGrassShell,
                Mesh3d(PARCEL_GRASS_MESH.clone()),
                MeshMaterial3d(PARCEL_GRASS_MATERIAL.clone()),
                Transform::from_translation(Vec3::new(
                    16. * shell_texturing_grass.parcel.x as f32 + 8.,
                    -0.05 + (shell_texturing_config.y_displacement * i as f32),
                    -(16. * shell_texturing_grass.parcel.y as f32) - 8.,
                )),
                MeshTag(i),
                NotShadowCaster,
            ));
        }
    });
}
