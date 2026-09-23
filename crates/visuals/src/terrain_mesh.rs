use std::sync::Arc;

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        primitives::Aabb,
    },
    tasks::Task,
};
use common::{
    structs::{LandscapeTerrain, PrimaryCamera, PrimaryUser},
    terrain::TerrainField,
    util::TaskExt,
};
use scene_runner::update_world::mesh_collider::SceneColliderData;

use crate::shell_texturing::{Ground, ParcelGrassShell, GROUND_FLAT_MESH, GROUND_MESH};

pub(super) struct TerrainMeshPlugin;

impl Plugin for TerrainMeshPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainMeshes>()
            .add_systems(Update, update.after(crate::terrain_loading::load))
            .add_systems(
                PostUpdate,
                crate::terrain_support::recover
                    .after(common::sets::PostUpdateSets::ColliderUpdate)
                    .before(common::sets::PostUpdateSets::PlayerUpdate),
            );
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct MeshKey {
    generation: u64,
    origin: [i32; 2],
    scale: i32,
}

#[derive(Resource, Default)]
pub(super) struct TerrainMeshes {
    generation: Option<u64>,
    requested: Option<MeshKey>,
    applied: Option<MeshKey>,
    task: Option<Task<Mesh>>,
    collision_requested: Option<MeshKey>,
    collision_task: Option<Task<Result<SceneColliderData, String>>>,
    pub collision_status: String,
    pub vertices: usize,
    pub status: String,
}

fn camera_key(generation: u64, position: Vec3) -> Option<MeshKey> {
    if !position.is_finite() || position.abs().max_element() > 1_000_000.0 {
        return None;
    }
    Some(MeshKey {
        generation,
        origin: [position.x, -position.z].map(|value| ((value / 16.0).floor() as i32 + 1) & !1),
        scale: (position.y.max(0.0) / 16.0) as i32 + 1,
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update(
    mut commands: Commands,
    mut terrain: ResMut<LandscapeTerrain>,
    mut state: ResMut<TerrainMeshes>,
    cameras: Query<&GlobalTransform, With<PrimaryCamera>>,
    players: Query<&GlobalTransform, With<PrimaryUser>>,
    mut ground: Query<(Entity, &mut Transform), With<Ground>>,
    ground_meshes: Query<(Entity, &Mesh3d), Or<(With<Ground>, With<ParcelGrassShell>)>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if state.generation != Some(terrain.generation) {
        *state = TerrainMeshes {
            generation: Some(terrain.generation),
            ..default()
        };
        terrain.rendered_generation = None;
        crate::shell_texturing::reset_ground_meshes(&mut meshes);
        refresh_ground_bounds(&mut commands, &ground_meshes);
        for (entity, mut transform) in &mut ground {
            transform.scale = Vec3::new(1024.0, 1.0, 1024.0);
            commands.entity(entity).remove::<SceneColliderData>();
        }
    }
    let Some(field) = terrain.field.as_ref() else {
        return;
    };
    let Ok(camera) = cameras.single() else {
        return;
    };
    let Some(key) = camera_key(terrain.generation, camera.translation()) else {
        return;
    };
    if ground.single().is_err() {
        return;
    }
    if state.requested != Some(key) {
        let field = Arc::clone(field);
        state.requested = Some(key);
        state.status = "building terrain mesh".into();
        state.task = Some(crate::spawn_cpu(move || build_mesh(&field, key)));
    }
    if let Some(mesh) = state.task.as_mut().and_then(TaskExt::complete) {
        state.task = None;
        let Ok((_, mut transform)) = ground.single_mut() else {
            return;
        };
        state.vertices = mesh.count_vertices();
        meshes.insert(GROUND_FLAT_MESH.id(), mesh.clone());
        meshes.insert(GROUND_MESH.id(), mesh);
        refresh_ground_bounds(&mut commands, &ground_meshes);
        transform.scale = Vec3::ONE;
        state.applied = Some(key);
        terrain.rendered_generation = Some(terrain.generation);
        state.status = "ready".into();
    }
    let Ok(player) = players.single() else {
        return;
    };
    let position = player.translation();
    let Some(collision_key) =
        camera_key(terrain.generation, Vec3::new(position.x, 0.0, position.z))
    else {
        return;
    };
    if state.collision_requested != Some(collision_key) {
        let field = Arc::clone(terrain.field.as_ref().unwrap());
        state.collision_requested = Some(collision_key);
        state.collision_status = "building player terrain collider".into();
        state.collision_task = Some(crate::spawn_cpu(move || {
            SceneColliderData::from_terrain_mesh(&build_surface(&field, collision_key, 32, false))
        }));
    }
    if let Some(result) = state.collision_task.as_mut().and_then(TaskExt::complete) {
        state.collision_task = None;
        match result {
            Ok(collider) => {
                commands.entity(ground.single().unwrap().0).insert(collider);
                state.collision_status = "ready".into();
            }
            Err(error) => {
                warn!("Terrain collider failed: {error}");
                state.collision_status = error;
            }
        }
    }
}

// Bounds are only computed for entities without one; a mesh replaced in place
// keeps its old box, which no longer matches once the ground is rescaled.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn refresh_ground_bounds(
    commands: &mut Commands,
    ground_meshes: &Query<(Entity, &Mesh3d), Or<(With<Ground>, With<ParcelGrassShell>)>>,
) {
    for (entity, mesh) in ground_meshes {
        if mesh.0 == GROUND_MESH || mesh.0 == GROUND_FLAT_MESH {
            commands.entity(entity).remove::<Aabb>();
        }
    }
}

fn normal(field: &TerrainField, x: f32, z: f32) -> Vec3 {
    let dx = field.height(x + 0.5, z) - field.height(x - 0.5, z);
    let dz = field.height(x, z + 0.5) - field.height(x, z - 0.5);
    Vec3::new(-dx, 1.0, dz).normalize()
}

fn build_mesh(field: &TerrainField, key: MeshKey) -> Mesh {
    build_surface(field, key, 16, true)
}

fn build_surface(field: &TerrainField, key: MeshKey, segments: u32, shading: bool) -> Mesh {
    let mut positions = Vec::<[f32; 3]>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut uv = Vec::<[f32; 2]>::new();
    let mut indices = Vec::<u32>::new();
    let minimum = field.min_parcel.map(|value| value as f32 * 16.0);
    let maximum = field.max_parcel.map(|value| (value + 1) as f32 * 16.0);
    let mut append = |tile: [i32; 2], scale: i32, ring: bool| {
        let start = [0, 1].map(|axis| {
            (i64::from(key.origin[axis]) + i64::from(tile[axis]) * i64::from(scale)) as f32 * 16.0
        });
        let width = scale as f32 * 16.0;
        if (0..2).any(|axis| start[axis] >= maximum[axis] || start[axis] + width <= minimum[axis]) {
            return;
        }
        let base = positions.len() as u32;
        let step = width / segments as f32;
        for z in 0..=segments {
            for x in 0..=segments {
                let world_x = (start[0] + x as f32 * step).clamp(minimum[0], maximum[0]);
                let world_z = (start[1] + z as f32 * step).clamp(minimum[1], maximum[1]);
                let mut height = field.height(world_x, world_z);
                let mut vertex_normal = if shading {
                    normal(field, world_x, world_z)
                } else {
                    Vec3::Y
                };
                let stitch_z = ring
                    && z % 2 == 1
                    && ((tile[0] == -2 && x == 0) || (tile[0] == 1 && x == segments));
                let stitch_x = ring
                    && x % 2 == 1
                    && ((tile[1] == -2 && z == 0) || (tile[1] == 1 && z == segments));
                if stitch_x || stitch_z {
                    let step_x = if stitch_x { step } else { 0.0 };
                    let step_z = if stitch_z { step } else { 0.0 };
                    let left = [
                        (world_x - step_x).clamp(minimum[0], maximum[0]),
                        (world_z - step_z).clamp(minimum[1], maximum[1]),
                    ];
                    let right = [
                        (world_x + step_x).clamp(minimum[0], maximum[0]),
                        (world_z + step_z).clamp(minimum[1], maximum[1]),
                    ];
                    height =
                        (field.height(left[0], left[1]) + field.height(right[0], right[1])) * 0.5;
                    if shading {
                        vertex_normal = (normal(field, left[0], left[1])
                            + normal(field, right[0], right[1]))
                        .normalize();
                    }
                }
                positions.push([world_x, height, -world_z]);
                if shading {
                    normals.push(vertex_normal.to_array());
                    uv.push([world_x / 16.0, world_z / 16.0]);
                }
            }
        }
        let row = segments + 1;
        for z in 0..segments {
            for x in 0..segments {
                let a = base + z * row + x;
                indices.extend_from_slice(&[a, a + 1, a + row, a + 1, a + row + 1, a + row]);
            }
        }
    };
    for x in -1..=0 {
        for z in -1..=0 {
            append([x, z], key.scale, false);
        }
    }
    let mut scale = key.scale;
    for _ in 0..16 {
        for x in -2..=1 {
            for z in -2..=1 {
                if !(-1..=0).contains(&x) || !(-1..=0).contains(&z) {
                    append([x, z], scale, true);
                }
            }
        }
        let covered = (0..2).all(|axis| {
            (i64::from(key.origin[axis]) - 2 * i64::from(scale)) as f32 * 16.0 <= minimum[axis]
                && (i64::from(key.origin[axis]) + 2 * i64::from(scale)) as f32 * 16.0
                    >= maximum[axis]
        });
        if covered {
            break;
        }
        let Some(next) = scale.checked_mul(2) else {
            break;
        };
        scale = next;
    }
    let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices));
    if shading {
        mesh.with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    } else {
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    #[test]
    fn rings_cover_terrain_without_out_of_bounds_or_nonfinite_vertices() {
        let parcels: Vec<_> = (0..20).flat_map(|x| (0..20).map(move |z| [x, z])).collect();
        let field = TerrainField::from_parcels(&parcels, true).unwrap();
        let mesh = build_mesh(&field, camera_key(1, Vec3::new(85.0, 12.0, -95.0)).unwrap());
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("missing positions");
        };
        assert!(positions.len() < 32_000);
        assert!(positions.iter().any(|position| position[1] > 1.0));
        assert!(positions
            .iter()
            .all(|position| position.iter().all(|value| value.is_finite())
                && (-64.0..=384.0).contains(&position[0])
                && (-384.0..=64.0).contains(&position[2])));
        for position in positions.iter().filter(|position| {
            position[0] >= 0.0
                && position[0] <= 304.0
                && position[2] <= 0.0
                && position[2] >= -304.0
        }) {
            assert_eq!(position[1], 0.0);
        }
        let mut collider = SceneColliderData::from_terrain_mesh(&mesh).unwrap();
        for position in positions
            .iter()
            .filter(|position| position[1] > 1.0)
            .step_by(37)
        {
            let point = Vec3::from_array(*position);
            let hit = collider
                .cast_ray_nearest(
                    point + Vec3::Y,
                    Vec3::NEG_Y,
                    2.0,
                    scene_runner::update_world::mesh_collider::GROUND_COLLISION_MASK,
                    false,
                    false,
                    None,
                )
                .unwrap();
            assert!((hit.position.y - point.y).abs() < 0.0001);
        }
    }

    #[test]
    fn a_new_generation_clears_the_old_mesh_and_collider() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .insert_resource(LandscapeTerrain {
                generation: 2,
                rendered_generation: Some(1),
                field: None,
            })
            .insert_resource(TerrainMeshes {
                generation: Some(1),
                ..default()
            })
            .add_systems(Update, update);
        let ground = app
            .world_mut()
            .spawn((Ground, Transform::IDENTITY, SceneColliderData::default()))
            .id();
        app.update();
        assert!(app.world().get::<SceneColliderData>(ground).is_none());
        assert_eq!(
            app.world().get::<Transform>(ground).unwrap().scale,
            Vec3::new(1024.0, 1.0, 1024.0)
        );
        assert_eq!(
            app.world()
                .resource::<LandscapeTerrain>()
                .rendered_generation,
            None
        );
    }

    #[test]
    fn clipped_ring_boundaries_have_one_height_per_position() {
        let parcels: Vec<_> = (-7..14)
            .flat_map(|x| (-3..16).map(move |z| [x, z]))
            .collect();
        let field = TerrainField::from_parcels(&parcels, true).unwrap();
        for y in [2.0, 35.0, 165.0] {
            let key = camera_key(1, Vec3::new(31.0, y, -79.0)).unwrap();
            let mesh = build_mesh(&field, key);
            let Some(VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("missing positions");
            };
            let mut heights = std::collections::HashMap::new();
            for [x, height, z] in positions {
                if let Some(previous) = heights.insert((x.to_bits(), z.to_bits()), *height) {
                    assert!(
                        (previous - height).abs() < 0.00001,
                        "camera Y={y}, position ({x}, {z}) has heights {previous} and {height}"
                    );
                }
            }
        }
    }
}
