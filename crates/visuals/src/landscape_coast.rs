use crate::{
    ground_visibility::GroundVisibility,
    landscape_coast_geometry::{self as geometry, SEA_LEVEL},
    landscape_rigid::RigidAssets,
};
use bevy::{
    asset::{embedded_asset, embedded_path},
    pbr::{ExtendedMaterial, MaterialExtension},
    pbr::{NotShadowCaster, NotShadowReceiver},
    platform::collections::HashMap,
    prelude::*,
    render::render_resource::AsBindGroup,
    render::render_resource::ShaderRef,
};
use common::structs::{LandscapeTerrain, PrimaryCamera};
use scene_runner::update_world::mesh_collider::SceneColliderData;

#[cfg(test)]
mod tests;

pub(super) struct LandscapeCoastPlugin;
impl Plugin for LandscapeCoastPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "landscape_water.wgsl");
        app.init_resource::<State>()
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_systems(Startup, setup)
            .add_systems(Update, update.after(crate::terrain_loading::load));
    }
}

type WaterMaterial = ExtendedMaterial<Water>;
#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct Water {
    #[uniform(100)]
    bounds: Vec4,
    #[texture(102)]
    #[sampler(103)]
    ripples: Handle<Image>,
}
impl MaterialExtension for Water {
    type Base = StandardMaterial;

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("landscape_water.wgsl").display()
            )
            .into(),
        )
    }
}

#[derive(Component)]
struct Ocean;
#[derive(Component)]
struct Cliff {
    key: (u32, i32),
}
#[derive(Component)]
struct Border;
#[derive(Resource)]
struct WaterAssets {
    mesh: Handle<Mesh>,
    material: Handle<WaterMaterial>,
}
#[derive(Default, Resource)]
struct State {
    key: Option<(u64, IVec2)>,
    generation: Option<u64>,
    bounds: Option<Vec4>,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.insert_resource(WaterAssets {
        mesh: meshes.add(Plane3d::default().mesh().size(32768.0, 32768.0)),
        material: materials.add(ExtendedMaterial {
            base: StandardMaterial {
                base_color: Color::srgb(0.065, 0.30, 0.36),
                perceptual_roughness: 0.26,
                reflectance: 0.38,
                // The open sea already reflects the atmospheric sky. Generic
                // scene-distance fog paints it into a solid gray horizon slab.
                fog_enabled: false,
                ..default()
            },
            extension: Water {
                bounds: Vec4::ZERO,
                ripples: images.add(crate::ocean_texture::image()),
            },
        }),
    });
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update(
    mut commands: Commands,
    mut state: ResMut<State>,
    terrain: Res<LandscapeTerrain>,
    visible: Res<GroundVisibility>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    water: Res<WaterAssets>,
    rigid: Res<RigidAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<WaterMaterial>>,
    mut rigid_materials: ResMut<Assets<crate::landscape_rigid::RigidMaterial>>,
    oceans: Query<Entity, With<Ocean>>,
    cliffs: Query<(Entity, &Cliff)>,
    borders: Query<Entity, With<Border>>,
) {
    let position = camera
        .single()
        .ok()
        .map(GlobalTransform::translation)
        .filter(|p| p.is_finite() && p.abs().max_element() < 1_000_000.0);
    let key = if visible.0 && terrain.field.is_some() {
        position.map(|p| (terrain.generation, (p.xz() / 64.0).floor().as_ivec2()))
    } else {
        None
    };
    if state.key == key {
        return;
    }
    state.key = key;
    let generation = key.map(|k| k.0);
    let changed = generation != state.generation;
    state.generation = generation;
    if changed || key.is_none() {
        for entity in &oceans {
            commands.entity(entity).despawn();
        }
        for entity in &borders {
            commands.entity(entity).despawn();
        }
        state.bounds = None;
    }
    let mut desired = HashMap::new();
    if let (Some(field), Some((_, cell)), Some(position)) = (terrain.field.as_ref(), key, position)
    {
        let bounds = geometry::bounds(field);
        state.bounds = Some(bounds);
        let center = Transform::from_xyz(cell.x as f32 * 64.0, SEA_LEVEL, cell.y as f32 * 64.0);
        if changed || oceans.is_empty() {
            materials.get_mut(&water.material).unwrap().extension.bounds = bounds;
            rigid_materials.get_mut(&rigid.1).unwrap().extension.bounds = bounds;
            commands.spawn((
                Name::new("Landscape ocean"),
                Ocean,
                Mesh3d(water.mesh.clone()),
                MeshMaterial3d(water.material.clone()),
                center,
                NotShadowCaster,
                NotShadowReceiver,
            ));
            match SceneColliderData::from_terrain_mesh(&geometry::boundary_collider(bounds)) {
                Ok(collider) => {
                    commands.spawn((
                        Name::new("Landscape borders"),
                        Border,
                        collider,
                        Transform::default(),
                    ));
                }
                Err(error) => error!("Landscape boundary collision failed: {error}"),
            }
        } else {
            for entity in &oceans {
                commands.entity(entity).insert(center);
            }
        }
        for key in geometry::near_chunks(bounds, position) {
            desired.insert(key, ());
        }
        for (entity, cliff) in &cliffs {
            if changed || !desired.contains_key(&cliff.key) {
                commands.entity(entity).despawn();
            } else {
                desired.remove(&cliff.key);
            }
        }
        for (side, chunk) in desired.keys().copied() {
            commands.spawn((
                Name::new("Landscape cliff"),
                Cliff { key: (side, chunk) },
                Mesh3d(meshes.add(geometry::cliff(field, side, chunk))),
                MeshMaterial3d(rigid.1.clone()),
                NotShadowCaster,
            ));
        }
    } else {
        for (entity, _) in &cliffs {
            commands.entity(entity).despawn();
        }
    }
}
