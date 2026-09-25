use crate::{
    ground_visibility::GroundVisibility,
    landscape_prop_geometry::{self, KINDS},
    landscape_rigid::RigidAssets,
    landscape_trees::TreeAssets,
    tree_geometry::random,
};
use bevy::{platform::collections::HashMap, prelude::*, render::primitives::Aabb};
use bevy_console::ConsoleCommand;
use common::{
    structs::{LandscapeTerrain, PrimaryCamera, PrimaryUser},
    terrain::TerrainField,
};
use console::DoAddConsoleCommand;
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    update_world::mesh_collider::SceneColliderData,
    vec3_to_parcel,
};

pub(super) struct LandscapePropsPlugin;
impl Plugin for LandscapePropsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<State>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (refresh.after(crate::terrain_loading::load), update_lods).chain(),
            )
            .add_console_command::<PropsCommand, _>(props_command);
    }
}

#[derive(Resource)]
struct Assets {
    meshes: Vec<[Handle<Mesh>; 2]>,
    bounds: Vec<Aabb>,
}

fn setup(mut commands: Commands, mut meshes: ResMut<bevy::asset::Assets<Mesh>>) {
    let mut bounds = Vec::new();
    let models = (0..KINDS)
        .map(|kind| {
            let models: [Mesh; 2] =
                std::array::from_fn(|lod| landscape_prop_geometry::mesh(kind, lod == 1));
            bounds.push(crate::tree_geometry::lod_bounds(&models, 0.5));
            models.map(|mesh| meshes.add(mesh))
        })
        .collect();
    commands.insert_resource(Assets {
        meshes: models,
        bounds,
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
    kind: usize,
    lod: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    key: Key,
    kind: usize,
    transform: Transform,
}

#[derive(Component)]
struct RockColliders;

#[derive(Resource)]
struct State {
    key: Option<(u64, IVec2)>,
    enabled: bool,
    dirty: bool,
    collision_rocks: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            key: None,
            enabled: true,
            dirty: false,
            collision_rocks: 0,
        }
    }
}

fn placements(parcel: IVec2, field: &TerrainField, pointers: &ScenePointers) -> Vec<Placement> {
    if !matches!(pointers.get(parcel), Some(PointerResult::Nothing)) {
        return Vec::new();
    }
    let pixel = parcel + IVec2::splat(field.size as i32 / 2);
    if !pixel.cmpge(IVec2::ZERO).all()
        || !pixel.cmplt(IVec2::splat(field.size as i32)).all()
        || field.pixels[pixel.y as usize * field.size + pixel.x as usize] == 0
    {
        return Vec::new();
    }
    let mut rock: Option<(Vec2, f32)> = None;
    (0..9)
        .filter_map(|slot| {
            let seed = (parcel.x as u32).wrapping_mul(73856093)
                ^ (parcel.y as u32).wrapping_mul(19349663)
                ^ (slot * 1709 + 0xab37);
            if random(seed) > 0.62 {
                return None;
            }
            let kind = if slot == 0 {
                0
            } else if slot < 3 {
                2
            } else {
                4
            } + usize::from(random(seed ^ 17) > 0.5);
            // Leave a clear arrival area and keep every prop inside its empty parcel.
            let mut local = Vec2::new(
                2.5 + random(seed ^ 251) * 11.0,
                2.5 + random(seed ^ 397) * 11.0,
            );
            if (2..4).contains(&kind) {
                if let Some((center, radius)) = rock {
                    // Reposition the existing shrub candidates around a valid
                    // stone, leaving other parcels open. Never add candidates
                    // or clamp an unsafe grouping onto the parcel boundary.
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
            if !field.contains(x, z)
                || Vec2::new(x, z).distance(parcel.as_vec2() * 16.0 + Vec2::splat(8.0)) < 2.0
            {
                return None;
            }
            let y = field.height(x, z);
            let slope = (field.height(x + 0.7, z) - field.height(x - 0.7, z))
                .abs()
                .max((field.height(x, z + 0.7) - field.height(x, z - 0.7)).abs());
            if slope > 0.9 {
                return None;
            }
            let scale = if kind < 2 {
                // Mostly small stones, with occasional substantial boulders.
                0.65 + random(seed ^ 601).powi(2) * 1.55
            } else if kind < 4 {
                0.55 + random(seed ^ 601) * 0.35
            } else {
                0.75 + random(seed ^ 601) * 0.5
            };
            if kind < 2
                && Vec2::new(x, z).distance(parcel.as_vec2() * 16.0 + Vec2::splat(8.0))
                    < 2.0 + scale
            {
                return None;
            }
            if kind < 2 {
                rock = Some((local, scale));
            }
            Some(Placement {
                key: Key { parcel, slot },
                kind,
                transform: Transform::from_xyz(x, y - 0.015, -z)
                    .with_rotation(Quat::from_rotation_y(
                        random(seed ^ 997) * std::f32::consts::TAU,
                    ))
                    .with_scale(Vec3::splat(scale)),
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn refresh(
    mut commands: Commands,
    mut state: ResMut<State>,
    terrain: Res<LandscapeTerrain>,
    visible: Res<GroundVisibility>,
    pointers: Res<ScenePointers>,
    assets: Res<Assets>,
    trees: Res<TreeAssets>,
    rigid: Res<RigidAssets>,
    meshes: Res<bevy::asset::Assets<Mesh>>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    props: Query<(Entity, &Prop)>,
    colliders: Query<Entity, With<RockColliders>>,
) {
    let position = player
        .single()
        .ok()
        .map(GlobalTransform::translation)
        .filter(|p| p.is_finite() && p.abs().max_element() < 1_000_000.0);
    let key = if state.enabled && visible.0 && terrain.field.is_some() {
        position.map(|p| (terrain.generation, vec3_to_parcel(p)))
    } else {
        None
    };
    if state.key == key && !state.dirty && !pointers.is_changed() && !visible.is_changed() {
        return;
    }
    state.dirty = false;
    let generation_changed = state.key.map(|k| k.0) != key.map(|k| k.0);
    state.key = key;
    let mut desired = HashMap::new();
    if let (Some((_, center)), Some(field)) = (key, terrain.field.as_ref()) {
        for z in -6i32..=6 {
            for x in -6i32..=6 {
                if x * x + z * z > 36 {
                    continue;
                }
                let parcel = center + IVec2::new(x, z);
                for prop in placements(parcel, field, &pointers) {
                    // Tiny flower clusters do not need to be submitted at tree distances.
                    if prop.kind >= 4 && x * x + z * z > 9 {
                        continue;
                    }
                    desired.insert(prop.key, prop);
                }
            }
        }
    }
    for entity in &colliders {
        commands.entity(entity).despawn();
    }
    let mut collision: Option<Mesh> = None;
    state.collision_rocks = 0;
    for prop in desired.values().filter(|p| {
        p.kind < 2 && position.is_some_and(|player| player.distance(p.transform.translation) < 40.0)
    }) {
        let Some(mesh) = meshes.get(&assets.meshes[prop.kind][1]) else {
            continue;
        };
        let mesh = mesh.clone().transformed_by(prop.transform);
        if let Some(collision) = &mut collision {
            collision.merge(&mesh).expect("identical rock layouts");
        } else {
            collision = Some(mesh);
        }
        state.collision_rocks += 1;
    }
    if let Some(mesh) = collision {
        match SceneColliderData::from_terrain_mesh(&mesh) {
            Ok(collider) => {
                commands.spawn((
                    Name::new("Landscape rock collisions"),
                    RockColliders,
                    collider,
                    Transform::default(),
                ));
            }
            Err(error) => error!("Landscape rock collision failed: {error}"),
        }
    }
    for (entity, prop) in &props {
        if generation_changed || !desired.contains_key(&prop.key) {
            commands.entity(entity).despawn();
        } else {
            desired.remove(&prop.key);
        }
    }
    for prop in desired.values() {
        let mut entity = commands.spawn((
            Name::new("Landscape detail"),
            Prop {
                key: prop.key,
                kind: prop.kind,
                lod: 1,
            },
            Mesh3d(assets.meshes[prop.kind][1].clone()),
            prop.transform,
            assets.bounds[prop.kind],
        ));
        if prop.kind < 2 {
            entity.insert(MeshMaterial3d(rigid.0.clone()));
        } else {
            entity.insert(MeshMaterial3d(trees.material.clone()));
        }
    }
}

fn update_lods(
    assets: Res<Assets>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut props: Query<(&Transform, &mut Prop, &mut Mesh3d)>,
) {
    let Ok(camera) = camera.single() else {
        return;
    };
    for (transform, mut prop, mut mesh) in &mut props {
        let distance = camera.translation().distance(transform.translation);
        let threshold = if prop.lod == 0 { 30.0 } else { 25.0 };
        let next = usize::from(distance > threshold);
        if next != prop.lod {
            prop.lod = next;
            mesh.0 = assets.meshes[prop.kind][next].clone();
        }
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/landscape_props")]
struct PropsCommand {
    enabled: Option<bool>,
}

fn props_command(
    mut input: ConsoleCommand<PropsCommand>,
    mut state: ResMut<State>,
    props: Query<(&Prop, &Mesh3d, &ViewVisibility)>,
    meshes: Res<bevy::asset::Assets<Mesh>>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Some(enabled) = command.enabled {
            if state.enabled != enabled {
                state.enabled = enabled;
                state.dirty = true;
            }
        }
        let mut counts = [0; 3];
        let mut vertices = 0;
        let mut visible = 0;
        for (prop, mesh, view) in &props {
            counts[prop.kind / 2] += 1;
            visible += usize::from(view.get());
            vertices += meshes.get(&mesh.0).map_or(0, Mesh::count_vertices);
        }
        input.reply_ok(format!("landscape_props: stones={} bushes={} flowers={} visible={visible} vertices={vertices} colliders={} enabled={}", counts[0], counts[1], counts[2], state.collision_rocks, state.enabled));
    }
}

#[cfg(test)]
mod tests;
