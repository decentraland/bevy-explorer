//! The coast around the terrain, after eordano's procedural landscape (#1342): cliffs dropping from
//! the ground to a sandy shelf, the sea, and invisible walls along the cliff crest. Unity places
//! cliff prefabs along the terrain bounds; this builds a rounded, varying cliff instead. The ground
//! and grass end just past the cliff crest (`beyond_coast` in coast_profile.wgsl).
//!
//! Coordinates in the geometry are unity x/z metres (bevy z = -unity z).

mod ripples;
#[cfg(test)]
mod tests;

use std::collections::HashSet;

use bevy::{
    asset::{embedded_asset, embedded_path, RenderAssetUsages},
    pbr::{ExtendedMaterial, MaterialExtension, NotShadowCaster, NotShadowReceiver},
    prelude::*,
    render::{
        mesh::PrimitiveTopology,
        render_resource::{AsBindGroup, ShaderRef},
    },
};
use common::{sets::PostUpdateSets, structs::PrimaryCamera};
use dcl_component::SceneEntityId;
use scene_runner::update_world::mesh_collider::{ColliderId, SceneColliderData};

use crate::terrain::{TerrainSet, TerrainSurface, NO_COAST};

pub const SEA_LEVEL: f32 = -19.0;
/// Cliffs are built in chunks of this length along each side.
const CHUNK: f32 = 64.0;
/// Cliff chunks within this distance of the camera are built.
const CLIFF_RANGE: f32 = 768.0;
/// Facets per half corner; ten per corner keep the waterline chord error under 11 cm.
const CORNER_STEPS: i32 = 5;
/// Rings of the cliff profile, from the crest down to below the sea.
const RINGS: usize = 7;
/// The ground's visual offset (shell_texturing), where the cliff crest meets it.
const GROUND_LEVEL: f32 = -0.05;
const WALL_HEIGHT: f32 = 50.0;
const WALL_THICKNESS: f32 = 10.0;
const OCEAN_SIZE: f32 = 32768.0;
/// The ocean follows the camera on this grid, so its ripples don't slide.
const OCEAN_SNAP: f32 = 64.0;

pub(crate) struct CoastPlugin;

impl Plugin for CoastPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "coast_surf.wgsl");
        embedded_asset!(app, "coast_cliff.wgsl");
        embedded_asset!(app, "coast_water.wgsl");

        app.init_resource::<CoastState>()
            .add_plugins(MaterialPlugin::<CliffMaterial>::default())
            .add_plugins(MaterialPlugin::<WaterMaterial>::default())
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                update
                    .after(TerrainSet)
                    .before(PostUpdateSets::ColliderUpdate),
            );
    }
}

fn shader(path: &std::path::Path) -> ShaderRef {
    ShaderRef::Path(format!("embedded://{}", path.display()).into())
}

type CliffMaterial = ExtendedMaterial<CliffExtension>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct CliffExtension {
    #[uniform(100)]
    bounds: Vec4,
}

impl MaterialExtension for CliffExtension {
    type Base = StandardMaterial;

    fn fragment_shader() -> ShaderRef {
        shader(&embedded_path!("coast_cliff.wgsl"))
    }
}

type WaterMaterial = ExtendedMaterial<WaterExtension>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct WaterExtension {
    #[uniform(100)]
    bounds: Vec4,
    #[texture(101)]
    #[sampler(102)]
    ripples: Handle<Image>,
}

impl MaterialExtension for WaterExtension {
    type Base = StandardMaterial;

    fn fragment_shader() -> ShaderRef {
        shader(&embedded_path!("coast_water.wgsl"))
    }
}

#[derive(Resource)]
struct CoastAssets {
    cliff: Handle<CliffMaterial>,
    water: Handle<WaterMaterial>,
}

#[derive(Resource, Default)]
struct CoastState {
    bounds: Option<Vec4>,
}

#[derive(Component)]
struct Ocean;

#[derive(Component)]
struct Cliff {
    key: (u32, i32),
    walls: u32,
}

#[derive(Component)]
struct CoastWalls;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cliffs: ResMut<Assets<CliffMaterial>>,
    mut waters: ResMut<Assets<WaterMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let cliff = cliffs.add(ExtendedMaterial {
        base: StandardMaterial {
            perceptual_roughness: 0.95,
            ..default()
        },
        extension: CliffExtension { bounds: NO_COAST },
    });
    let water = waters.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color: Color::srgb(0.065, 0.30, 0.36),
            perceptual_roughness: 0.26,
            reflectance: 0.38,
            // the open sea reflects the sky; distance fog would paint a grey slab on the horizon
            fog_enabled: false,
            ..default()
        },
        extension: WaterExtension {
            bounds: NO_COAST,
            ripples: images.add(ripples::image()),
        },
    });
    commands.spawn((
        Name::new("Coast ocean"),
        Ocean,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(OCEAN_SIZE, OCEAN_SIZE))),
        MeshMaterial3d(water.clone()),
        Transform::from_xyz(0.0, SEA_LEVEL, 0.0),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
    ));
    commands.spawn((
        Name::new("Coast walls"),
        CoastWalls,
        SceneColliderData::default(),
        Transform::default(),
    ));
    commands.insert_resource(CoastAssets { cliff, water });
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    surface: Res<TerrainSurface>,
    mut state: ResMut<CoastState>,
    assets: Res<CoastAssets>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cliff_materials: ResMut<Assets<CliffMaterial>>,
    mut water_materials: ResMut<Assets<WaterMaterial>>,
    mut ocean: Query<(&mut Transform, &mut Visibility), With<Ocean>>,
    cliffs: Query<(Entity, &Cliff)>,
    walls: Single<&mut SceneColliderData, With<CoastWalls>>,
) {
    let bounds = surface.coast();
    let changed = bounds != state.bounds;
    if changed {
        state.bounds = bounds;
        let uniform = bounds.unwrap_or(NO_COAST);
        if let Some(material) = cliff_materials.get_mut(&assets.cliff) {
            material.extension.bounds = uniform;
        }
        if let Some(material) = water_materials.get_mut(&assets.water) {
            material.extension.bounds = uniform;
        }
    }
    let mut walls = walls.into_inner();

    let position = camera
        .single()
        .ok()
        .map(GlobalTransform::translation)
        .filter(|position| position.is_finite());
    let (Some(bounds), Some(position)) = (bounds, position) else {
        for (_, mut visibility) in &mut ocean {
            visibility.set_if_neq(Visibility::Hidden);
        }
        for (entity, cliff) in &cliffs {
            remove_walls(&mut walls, cliff);
            commands.entity(entity).despawn();
        }
        return;
    };

    for (mut transform, mut visibility) in &mut ocean {
        let centre = (position.xz() / OCEAN_SNAP).round() * OCEAN_SNAP;
        let translation = Vec3::new(centre.x, SEA_LEVEL, centre.y);
        if transform.translation != translation {
            transform.translation = translation;
        }
        visibility.set_if_neq(Visibility::Inherited);
    }

    let mut desired: HashSet<_> = near_chunks(bounds, position).into_iter().collect();
    for (entity, cliff) in &cliffs {
        if changed || !desired.remove(&cliff.key) {
            remove_walls(&mut walls, cliff);
            commands.entity(entity).despawn();
        }
    }
    for (side, chunk) in desired {
        let crest_walls = crest_walls(bounds, side, chunk);
        for (index, (centre, half_extents, rotation)) in crest_walls.iter().enumerate() {
            let id = wall_id((side, chunk), index as u32);
            walls.set_wall(&id, *centre, *half_extents, *rotation);
        }
        commands.spawn((
            Name::new("Coast cliff"),
            Cliff {
                key: (side, chunk),
                walls: crest_walls.len() as u32,
            },
            Mesh3d(meshes.add(cliff_mesh(bounds, side, chunk))),
            MeshMaterial3d(assets.cliff.clone()),
            Transform::default(),
            NotShadowCaster,
        ));
    }
}

/// Deterministic value in [0, 1] (pcg hash).
fn random(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

/// Cliff crest and waterline distances outward from `edge` (coast_profile.wgsl).
fn coast_profile(edge: Vec2) -> Vec2 {
    let a = (edge.dot(Vec2::new(0.013, 0.019)) + 0.7).sin();
    let b = (edge.dot(Vec2::new(0.041, -0.027)) + 2.2).sin();
    let c = (edge.dot(Vec2::new(0.007, -0.011)) - 0.4).sin();
    let crest = 8.0 + 4.0 * a + 2.0 * b;
    Vec2::new(crest, crest + 18.0 + 2.0 * c + b)
}

/// The point `along` side `side` of the bounds, going counter-clockwise from the min corner.
fn edge(bounds: Vec4, side: u32, along: f32) -> Vec2 {
    match side {
        0 => Vec2::new((bounds.x + along).min(bounds.z), bounds.y),
        1 => Vec2::new(bounds.z, (bounds.y + along).min(bounds.w)),
        2 => Vec2::new((bounds.z - along).max(bounds.x), bounds.w),
        _ => Vec2::new(bounds.x, (bounds.w - along).max(bounds.y)),
    }
}

fn side_length(bounds: Vec4, side: u32) -> f32 {
    if side.is_multiple_of(2) {
        bounds.z - bounds.x
    } else {
        bounds.w - bounds.y
    }
}

/// Outward normal of `side`, turned `step` corner facets towards the next side (positive) or the
/// previous one (negative).
fn arc_normal(side: u32, step: i32) -> Vec2 {
    let normal = [Vec2::NEG_Y, Vec2::X, Vec2::Y, Vec2::NEG_X][side as usize];
    let tangent = Vec2::new(-normal.y, normal.x);
    if step.abs() == CORNER_STEPS {
        // exact at the shared 45 degree end, so both halves of a corner meet without a crack
        return (normal + tangent * step.signum() as f32) * std::f32::consts::FRAC_1_SQRT_2;
    }
    let angle = step as f32 * (std::f32::consts::FRAC_PI_4 / CORNER_STEPS as f32);
    let (sine, cosine) = angle.sin_cos();
    normal * cosine + tangent * sine
}

/// Cliff chunks (side, index) within `CLIFF_RANGE` of the camera (bevy position).
fn near_chunks(bounds: Vec4, camera: Vec3) -> Vec<(u32, i32)> {
    let camera = Vec2::new(camera.x, -camera.z);
    let mut chunks = Vec::new();
    for side in 0..4 {
        let length = side_length(bounds, side);
        let start = edge(bounds, side, 0.0);
        let end = edge(bounds, side, length);
        let along = (camera - start)
            .dot((end - start).normalize())
            .clamp(0.0, length);
        if camera.distance(start.lerp(end, along / length)) > CLIFF_RANGE {
            continue;
        }
        let first = ((along - CLIFF_RANGE).max(0.0) / CHUNK).floor() as i32;
        let last = (((along + CLIFF_RANGE).min(length) / CHUNK).ceil() as i32 - 1).max(first);
        chunks.extend((first..=last).map(|chunk| (side, chunk)));
    }
    chunks
}

/// Cliff profile going out from `edge` along `outward`: the crest at ground level, fractured rock
/// with a ledge, a sandy shelf, the waterline and the slope below it.
fn profile(edge: Vec2, outward: Vec2) -> [Vec3; RINGS] {
    let seed = (edge.x as i32 as u32).wrapping_mul(1337) ^ (edge.y as i32 as u32).wrapping_mul(733);
    let Vec2 { x: crest, y: shore } = coast_profile(edge);
    let toe = (shore - crest - 18.0) * 0.4;
    let ledge = ((crest - 6.0) / 7.0).clamp(0.0, 1.0);
    let offsets = [
        crest,
        crest + 1.8 + ledge * 2.2,
        crest + 4.2 + ledge * 0.7,
        crest + 5.7 + toe,
        crest + 7.5 + toe,
        shore,
        shore + 6.0,
    ];
    let heights = [
        GROUND_LEVEL,
        -6.2 + ledge * 1.8,
        -11.5,
        -16.4,
        -17.3,
        SEA_LEVEL,
        SEA_LEVEL - 4.0,
    ];
    std::array::from_fn(|ring| {
        // the rock rings are jittered; the crest, shelf edge and waterline follow the profile
        let noise = random(seed ^ ((ring + 1) as u32).wrapping_mul(971));
        let fracture = (1..4).contains(&ring);
        let offset = offsets[ring] + if fracture { (noise - 0.5) * 0.8 } else { 0.0 };
        let drop = match ring {
            1 | 2 => noise * 1.3,
            3 => noise * 0.7,
            _ => 0.0,
        };
        let horizontal = edge + outward * offset;
        Vec3::new(horizontal.x, heights[ring] - drop, -horizontal.y)
    })
}

/// Profiles along a chunk every 4 m, plus the half corners at either end of the side.
fn columns(bounds: Vec4, side: u32, chunk: i32) -> Vec<[Vec3; RINGS]> {
    let length = side_length(bounds, side);
    let start = chunk as f32 * CHUNK;
    let end = (start + CHUNK).min(length);
    let mut segments = ((end - start) / 4.0).ceil() as usize;
    // a tiny realm can put both corners in one chunk
    if start == 0.0 && end == length {
        segments = segments.min(12);
    }
    let mut columns = Vec::with_capacity(segments + 1 + 2 * CORNER_STEPS as usize);
    if start == 0.0 {
        let anchor = edge(bounds, side, 0.0);
        columns.extend((-CORNER_STEPS..0).map(|step| profile(anchor, arc_normal(side, step))));
    }
    columns.extend((0..=segments).map(|step| {
        let along = start + (end - start) * step as f32 / segments as f32;
        profile(edge(bounds, side, along), arc_normal(side, 0))
    }));
    if end == length {
        let anchor = edge(bounds, side, length);
        columns.extend((1..=CORNER_STEPS).map(|step| profile(anchor, arc_normal(side, step))));
    }
    columns
}

/// Flat-shaded cliff and sand for one chunk, with vertex colours.
fn cliff_mesh(bounds: Vec4, side: u32, chunk: i32) -> Mesh {
    let columns = columns(bounds, side, chunk);
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    for (step, pair) in columns.windows(2).enumerate() {
        let [previous, next] = [pair[0], pair[1]];
        for ring in 0..RINGS - 1 {
            let [a, b, c, d] = [
                previous[ring],
                next[ring],
                previous[ring + 1],
                next[ring + 1],
            ];
            let color = if ring >= 3 {
                Vec3::new(0.81, 0.65, 0.48)
            } else {
                let seed = (chunk as u32).wrapping_mul(773)
                    ^ step as u32
                    ^ ((ring + 1) as u32).wrapping_mul(337);
                Vec3::new(0.43, 0.41, 0.40) * (0.86 + random(seed) * 0.18)
            };
            let color = Color::srgb(color.x, color.y, color.z)
                .to_linear()
                .to_f32_array();
            for points in [[a, c, b], [b, c, d]] {
                let normal = (points[1] - points[0]).cross(points[2] - points[0]);
                if normal.length_squared() < 1e-6 {
                    continue;
                }
                let normal = normal.normalize().to_array();
                for point in points {
                    positions.push(point.to_array());
                    normals.push(normal);
                    colors.push(color);
                }
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

fn wall_id((side, chunk): (u32, i32), index: u32) -> ColliderId {
    ColliderId::new(
        SceneEntityId::ROOT,
        Some(format!("coast wall {side} {chunk}")),
        index,
    )
}

fn remove_walls(walls: &mut SceneColliderData, cliff: &Cliff) {
    for index in 0..cliff.walls {
        walls.remove_collider(&wall_id(cliff.key, index));
    }
}

/// Centre, half extents and rotation (bevy) of one invisible wall per crest segment of a chunk:
/// `WALL_HEIGHT` high and `WALL_THICKNESS` thick, with its inner face on the crest, like
/// unity-explorer's border colliders but following the cliff edge.
fn crest_walls(bounds: Vec4, side: u32, chunk: i32) -> Vec<(Vec3, Vec3, Quat)> {
    let columns = columns(bounds, side, chunk);
    columns
        .windows(2)
        .filter_map(|pair| {
            let [a, b] = [pair[0][0], pair[1][0]].map(|point| point.with_y(0.0));
            let along = (b - a).normalize_or_zero();
            if along == Vec3::ZERO {
                return None;
            }
            // the chords never turn far between columns, so the waterline marks outward
            let outward = along.cross(Vec3::Y);
            let seaward = (pair[0][5] + pair[1][5] - pair[0][0] - pair[1][0]).with_y(0.0);
            let outward = if outward.dot(seaward) < 0.0 {
                -outward
            } else {
                outward
            };
            let rotation = Quat::from_mat3(&Mat3::from_cols(along, Vec3::Y, along.cross(Vec3::Y)));
            let centre =
                (a + b) * 0.5 + outward * WALL_THICKNESS * 0.5 + Vec3::Y * WALL_HEIGHT * 0.5;
            let half_extents = Vec3::new(a.distance(b), WALL_HEIGHT, WALL_THICKNESS) * 0.5;
            Some((centre, half_extents, rotation))
        })
        .collect()
}
