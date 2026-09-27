//! Rocks and bushes on empty parcels, placed like the trees. Bushes share the tree material;
//! rocks are solid.

use bevy::{platform::collections::HashMap, prelude::*, render::primitives::Aabb};
use common::{sets::PostUpdateSets, structs::PrimaryUser};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    update_world::{gltf_container::mesh_to_parry_shape, mesh_collider::SharedShape},
    vec3_to_parcel,
};

use crate::{
    solids::Solid,
    terrain::{TerrainSet, TerrainSurface},
    trees::{
        geometry::{random, tree_bounds},
        TreeAssets,
    },
};
use geometry::{mesh, rock_scale, rock_size, KINDS, ROCKS, ROCK_SCALES};

mod geometry;
#[cfg(test)]
mod tests;

/// Props spawn on parcels within this many parcels of the player's parcel.
const SPAWN_RANGE: i32 = 6;
/// Spawned props are kept out to this range.
const KEEP_RANGE: i32 = SPAWN_RANGE + 1;
/// Props sit this far into the ground.
const SINK: f32 = 0.015;
/// Largest height difference 0.7 m either side of a prop.
const MAX_SLOPE: f32 = 0.9;
/// Sway padding for the visibility bounds.
const WIND_PADDING: f32 = 0.5;

pub(crate) struct PropsPlugin;

impl Plugin for PropsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            PostUpdate,
            update_props
                .after(TerrainSet)
                .before(PostUpdateSets::ColliderUpdate),
        );
    }
}

#[derive(Resource)]
struct PropAssets {
    meshes: Vec<Handle<Mesh>>,
    bounds: Vec<Aabb>,
    /// Rock collision shapes by kind and size.
    shapes: Vec<Vec<SharedShape>>,
    rock: Handle<StandardMaterial>,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let models: Vec<_> = (0..KINDS).map(mesh).collect();
    let shapes = models[..ROCKS]
        .iter()
        .map(|model| {
            (0..ROCK_SCALES)
                .map(|size| {
                    let scaled = model.clone().scaled_by(Vec3::splat(rock_scale(size)));
                    mesh_to_parry_shape(&scaled).expect("rock mesh has triangles")
                })
                .collect()
        })
        .collect();
    commands.insert_resource(PropAssets {
        bounds: models
            .iter()
            .map(|mesh| tree_bounds(mesh, WIND_PADDING))
            .collect(),
        meshes: models.into_iter().map(|mesh| meshes.add(mesh)).collect(),
        shapes,
        rock: materials.add(StandardMaterial {
            perceptual_roughness: 0.95,
            ..default()
        }),
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    parcel: IVec2,
    slot: u32,
}

#[derive(Component)]
struct Prop {
    key: Key,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    key: Key,
    kind: usize,
    /// Rock size, 0 for bushes.
    size: usize,
    transform: Transform,
}

/// The parcel's props: a rock, and bushes grouped around it. `empty` says whether the parcel is
/// confirmed empty landscape; props stay inside it. `height` is the terrain at Unity x/z.
fn placements(
    parcel: IVec2,
    empty: impl Fn(IVec2) -> bool,
    height: impl Fn(f32, f32) -> f32,
) -> Vec<Placement> {
    if !empty(parcel) {
        return Vec::new();
    }
    let center = parcel.as_vec2() * 16.0 + Vec2::splat(8.0);
    let mut rock: Option<(Vec2, f32)> = None;
    (0..3)
        .filter_map(|slot| {
            let seed = (parcel.x as u32).wrapping_mul(73856093)
                ^ (parcel.y as u32).wrapping_mul(19349663)
                ^ (slot * 1709 + 0xab37);
            if random(seed) > 0.62 {
                return None;
            }
            let kind = if slot == 0 { 0 } else { ROCKS } + usize::from(random(seed ^ 17) > 0.5);
            // Leave a clear arrival area and keep every prop inside its empty parcel.
            let mut local = Vec2::new(
                2.5 + random(seed ^ 251) * 11.0,
                2.5 + random(seed ^ 397) * 11.0,
            );
            if kind >= ROCKS {
                if let Some((center, radius)) = rock {
                    // Group the bushes around a placed rock, dropping any that would leave
                    // the parcel.
                    let angle = random(seed ^ 0x914f) * std::f32::consts::TAU;
                    let distance = radius + 1.0 + random(seed ^ 0x69e3) * 0.6;
                    local = center + Vec2::new(angle.cos(), angle.sin()) * distance;
                    if local.min_element() < 2.5 || local.max_element() > 13.5 {
                        return None;
                    }
                }
            }
            let world = parcel.as_vec2() * 16.0 + local;
            let (x, z) = (world.x, world.y);
            if world.distance(center) < 2.0 {
                return None;
            }
            let y = height(x, z);
            let slope = (height(x + 0.7, z) - height(x - 0.7, z))
                .abs()
                .max((height(x, z + 0.7) - height(x, z - 0.7)).abs());
            if slope > MAX_SLOPE {
                return None;
            }
            let (size, scale) = if kind < ROCKS {
                // Mostly small stones, with occasional substantial boulders.
                let size = rock_size(0.65 + random(seed ^ 601).powi(2) * 1.55);
                (size, rock_scale(size))
            } else {
                (0, 0.55 + random(seed ^ 601) * 0.35)
            };
            if kind < ROCKS {
                if world.distance(center) < 2.0 + scale {
                    return None;
                }
                rock = Some((local, scale));
            }
            Some(Placement {
                key: Key { parcel, slot },
                kind,
                size,
                transform: Transform::from_xyz(x, y - SINK, -z)
                    .with_rotation(Quat::from_rotation_y(
                        random(seed ^ 997) * std::f32::consts::TAU,
                    ))
                    .with_scale(Vec3::splat(scale)),
            })
        })
        .collect()
}

/// Spawn and drop props around the player's parcel, and keep them on the eased terrain.
#[allow(clippy::too_many_arguments)]
fn update_props(
    mut commands: Commands,
    surface: Res<TerrainSurface>,
    pointers: Res<ScenePointers>,
    assets: Res<PropAssets>,
    trees: Res<TreeAssets>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    mut props: Query<(Entity, &Prop, &mut Transform)>,
    mut last_parcel: Local<Option<IVec2>>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let position = player.translation();
    if !position.is_finite() {
        return;
    }
    let centre = vec3_to_parcel(position);
    let changes = surface.changes();

    if *last_parcel != Some(centre)
        || pointers.is_changed()
        || changes.relayout
        || changes.targets.is_some()
    {
        *last_parcel = Some(centre);
        // unknown parcels stay clear until the realm confirms they are empty
        let empty = |parcel| {
            surface.contains(parcel) && matches!(pointers.get(parcel), Some(PointerResult::Nothing))
        };
        let mut desired = HashMap::new();
        for y in -KEEP_RANGE..=KEEP_RANGE {
            for x in -KEEP_RANGE..=KEEP_RANGE {
                let offset = IVec2::new(x, y);
                if offset.length_squared() > KEEP_RANGE * KEEP_RANGE {
                    continue;
                }
                for prop in placements(centre + offset, empty, |x, z| surface.height(x, z)) {
                    desired.insert(prop.key, prop);
                }
            }
        }
        for (entity, prop, _) in &props {
            if desired.remove(&prop.key).is_none() {
                commands.entity(entity).despawn();
            }
        }
        for (key, prop) in desired {
            if (key.parcel - centre).length_squared() > SPAWN_RANGE * SPAWN_RANGE {
                continue;
            }
            let mut entity = commands.spawn((
                Name::new("Landscape prop"),
                Prop { key },
                Mesh3d(assets.meshes[prop.kind].clone()),
                prop.transform,
                assets.bounds[prop.kind],
            ));
            if prop.kind < ROCKS {
                entity.insert((
                    MeshMaterial3d(assets.rock.clone()),
                    Solid::Shape(assets.shapes[prop.kind][prop.size].clone()),
                ));
            } else {
                entity.insert(MeshMaterial3d(trees.material.clone()));
            }
        }
    }

    // parcels whose values reach a prop's parcel through bilinear sampling
    let resnap = |parcel: IVec2| {
        changes.relayout
            || changes.params
            || changes.eased.is_some_and(|(min, max)| {
                (min - 1).cmple(parcel).all() && (max + 1).cmpge(parcel).all()
            })
    };
    for (_, prop, mut transform) in &mut props {
        if !resnap(prop.key.parcel) {
            continue;
        }
        let y = surface.height(transform.translation.x, -transform.translation.z) - SINK;
        if transform.translation.y != y {
            transform.translation.y = y;
        }
    }
}
