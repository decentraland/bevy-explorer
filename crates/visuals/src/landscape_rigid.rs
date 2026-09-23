//! Opaque landscape surfaces do not need the leaf atlas or wind/cutout passes.
use crate::grass_look::GrassLook;
use bevy::{
    asset::{embedded_asset, embedded_path},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline},
    prelude::*,
    render::mesh::MeshVertexBufferLayoutRef,
    render::render_resource::ShaderRef,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
};
use common::structs::ParcelGrassConfig;

pub(super) struct LandscapeRigidPlugin;

impl Plugin for LandscapeRigidPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "landscape_rigid.wgsl");
        embedded_asset!(app, "coast_profile.wgsl");
        embedded_asset!(app, "coast_surf.wgsl");
        app.add_plugins(MaterialPlugin::<RigidMaterial>::default())
            .add_systems(Startup, setup)
            .add_systems(Update, sync_coast_palette);
    }
}

pub(super) type RigidMaterial = ExtendedMaterial<RigidLighting>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(RigidKey)]
pub(super) struct RigidLighting {
    coast: bool,
    #[uniform(100)]
    pub(super) bounds: Vec4,
    #[storage(101, read_only)]
    ambient: Handle<bevy::render::storage::ShaderStorageBuffer>,
    #[uniform(102)]
    root: LinearRgba,
    #[uniform(103)]
    tip: LinearRgba,
    #[uniform(104)]
    detail: Vec4,
    #[texture(108)]
    #[sampler(109)]
    meadow: Handle<Image>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct RigidKey(bool);

impl From<&RigidLighting> for RigidKey {
    fn from(material: &RigidLighting) -> Self {
        Self(material.coast)
    }
}

impl MaterialExtension for RigidLighting {
    type Base = StandardMaterial;

    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if key.bind_group_data.0 {
            if let Some(fragment) = &mut descriptor.fragment {
                fragment.shader_defs.push("LANDSCAPE_COAST".into());
            }
        }
        Ok(())
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("landscape_rigid.wgsl").display()
            )
            .into(),
        )
    }
}

#[derive(Resource)]
pub(super) struct RigidAssets(pub Handle<RigidMaterial>, pub Handle<RigidMaterial>);

pub(super) fn setup(mut commands: Commands, mut materials: ResMut<Assets<RigidMaterial>>) {
    let rock = ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: RigidLighting {
            coast: false,
            bounds: Vec4::ZERO,
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
            root: Color::Srgba(ParcelGrassConfig::ROOT_COLOR).to_linear(),
            tip: Color::Srgba(ParcelGrassConfig::TIP_COLOR).to_linear(),
            detail: GrassLook::default().ground(),
            meadow: crate::meadow_texture::MEADOW,
        },
    };
    let mut coast = rock.clone();
    coast.extension.coast = true;
    commands.insert_resource(RigidAssets(materials.add(rock), materials.add(coast)));
}

fn sync_coast_palette(
    grass: Res<ParcelGrassConfig>,
    look: Res<GrassLook>,
    assets: Res<RigidAssets>,
    mut materials: ResMut<Assets<RigidMaterial>>,
) {
    if !grass.is_changed() && !look.is_changed() {
        return;
    }
    if let Some(material) = materials.get_mut(&assets.1) {
        material.extension.root = grass.root_color.to_linear();
        material.extension.tip = if grass.layers == 0 {
            grass.root_color
        } else {
            grass.tip_color
        }
        .to_linear();
        material.extension.detail = look.ground();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_resource::Face;

    #[test]
    fn rigid_surfaces_are_opaque_backface_culled_and_share_only_the_existing_meadow() {
        let mut app = App::new();
        app.init_resource::<Assets<RigidMaterial>>()
            .add_systems(Startup, setup);
        app.update();
        let assets = app.world().resource::<RigidAssets>();
        let materials = app.world().resource::<Assets<RigidMaterial>>();
        let material = materials.get(&assets.0).unwrap();
        assert_eq!(material.base.alpha_mode, AlphaMode::Opaque);
        assert_eq!(material.base.cull_mode, Some(Face::Back));
        assert!(!material.base.double_sided);
        assert!(material.base.base_color_texture.is_none());
        assert!(material.base.normal_map_texture.is_none());
        assert_eq!(material.extension.meadow, crate::meadow_texture::MEADOW);
        assert!(!RigidKey::from(&material.extension).0);
        assert!(RigidKey::from(&materials.get(&assets.1).unwrap().extension).0);
        assert!(matches!(RigidLighting::vertex_shader(), ShaderRef::Default));
        assert!(matches!(
            RigidLighting::prepass_vertex_shader(),
            ShaderRef::Default
        ));
    }

    #[test]
    fn exterior_meadow_tracks_grass_palette_and_flat_color_mode_without_changing_rocks() {
        let mut app = App::new();
        app.init_resource::<Assets<RigidMaterial>>()
            .init_resource::<ParcelGrassConfig>()
            .init_resource::<GrassLook>()
            .add_systems(Startup, setup)
            .add_systems(Update, sync_coast_palette);
        app.update();
        let rock = app.world().resource::<RigidAssets>().0.clone();
        let coast = app.world().resource::<RigidAssets>().1.clone();
        let original_rock = app
            .world()
            .resource::<Assets<RigidMaterial>>()
            .get(&rock)
            .unwrap()
            .extension
            .root;
        {
            let mut grass = app.world_mut().resource_mut::<ParcelGrassConfig>();
            grass.root_color = Color::srgb(0.3, 0.2, 0.1);
            grass.tip_color = Color::srgb(0.8, 0.6, 0.2);
            grass.layers = 0;
        }
        app.update();
        let materials = app.world().resource::<Assets<RigidMaterial>>();
        let material = &materials.get(&coast).unwrap().extension;
        assert_eq!(material.root, Color::srgb(0.3, 0.2, 0.1).to_linear());
        assert_eq!(material.tip, material.root);
        assert_eq!(materials.get(&rock).unwrap().extension.root, original_rock);
        app.world_mut().resource_mut::<ParcelGrassConfig>().layers = 1;
        app.update();
        assert_eq!(
            app.world()
                .resource::<Assets<RigidMaterial>>()
                .get(&coast)
                .unwrap()
                .extension
                .tip,
            Color::srgb(0.8, 0.6, 0.2).to_linear()
        );
    }
}
