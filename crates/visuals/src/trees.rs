//! Procedural trees on empty parcels. Placement is a per-parcel hash, heights follow the eased
//! terrain, and trunks are solid.

use bevy::{
    asset::{embedded_asset, embedded_path},
    pbr::{ExtendedMaterial, MaterialExtension},
    platform::collections::HashMap,
    prelude::*,
    render::{
        primitives::Aabb,
        render_resource::{AsBindGroup, ShaderRef},
    },
};
use common::{sets::PostUpdateSets, structs::PrimaryUser};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    vec3_to_parcel,
};

use crate::{
    solids::Solid,
    terrain::{TerrainSet, TerrainSurface},
};
use geometry::{random, tree_bounds, tree_mesh, trunk_axis, CROWN_RADIUS, KINDS, TRUNK_RADIUS};

pub(crate) mod geometry;
mod leaves;
#[cfg(test)]
mod tests;

/// Trees spawn on parcels within this many parcels of the player's parcel.
const SPAWN_RANGE: i32 = 11;
/// Spawned trees are kept out to this range.
const KEEP_RANGE: i32 = SPAWN_RANGE + 1;
/// Trees sit this far into the ground.
const SINK: f32 = 0.06;
/// Largest height difference 1.5 m either side of a trunk.
const MAX_SLOPE: f32 = 0.85;
/// Sway padding for the visibility bounds.
const WIND_PADDING: f32 = 0.4;

pub(crate) struct TreesPlugin;

impl Plugin for TreesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tree_wind.wgsl");
        embedded_asset!(app, "soft_shadows.wgsl");
        app.add_plugins((
            MaterialPlugin::<TreeMaterial>::default(),
            MaterialPlugin::<SoftShadowMaterial>::default(),
        ))
        .add_systems(Startup, setup)
        .add_systems(
            PostUpdate,
            update_trees
                .after(TerrainSet)
                .before(PostUpdateSets::ColliderUpdate),
        );
    }
}

pub(crate) type TreeMaterial = ExtendedMaterial<TreeWind>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(crate) struct TreeWind {
    #[uniform(100)]
    settings: Vec4,
}

impl MaterialExtension for TreeWind {
    type Base = StandardMaterial;

    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(format!("embedded://{}", embedded_path!("tree_wind.wgsl").display()).into())
    }
    fn prepass_vertex_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn deferred_vertex_shader() -> ShaderRef {
        Self::vertex_shader()
    }
    fn fragment_shader() -> ShaderRef {
        soft_shadows_fragment()
    }
}

pub(crate) fn soft_shadows_fragment() -> ShaderRef {
    ShaderRef::Path(
        format!(
            "embedded://{}",
            embedded_path!("soft_shadows.wgsl").display()
        )
        .into(),
    )
}

/// Plain `StandardMaterial` with softened sun shadows, for rocks.
pub(crate) type SoftShadowMaterial = ExtendedMaterial<SoftShadows>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(crate) struct SoftShadows {}

impl MaterialExtension for SoftShadows {
    type Base = StandardMaterial;

    fn fragment_shader() -> ShaderRef {
        soft_shadows_fragment()
    }
}

#[derive(Resource)]
pub(crate) struct TreeAssets {
    meshes: Vec<Handle<Mesh>>,
    bounds: Vec<Aabb>,
    /// Also used by bushes.
    pub(crate) material: Handle<TreeMaterial>,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TreeMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let models: Vec<_> = (0..KINDS).map(tree_mesh).collect();
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color_texture: Some(images.add(leaves::image())),
            alpha_mode: AlphaMode::Mask(0.5),
            cull_mode: None,
            // leaf cards carry crown-volume normals, which back faces must not flip
            double_sided: false,
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        },
        extension: TreeWind {
            settings: Vec4::new(0.18, 0.018, 0.0, 0.0),
        },
    });
    commands.insert_resource(TreeAssets {
        bounds: models
            .iter()
            .map(|mesh| tree_bounds(mesh, WIND_PADDING))
            .collect(),
        meshes: models.into_iter().map(|mesh| meshes.add(mesh)).collect(),
        material,
    });
}

#[derive(Component)]
struct Tree {
    parcel: IVec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    kind: usize,
    transform: Transform,
}

fn grove_density(parcel: IVec2) -> f32 {
    // A continuous, world-anchored 64 m field groups the per-parcel
    // candidates into groves and clearings.
    let position = (parcel.as_vec2() + Vec2::splat(0.5)) / 4.0;
    let cell = position.floor().as_ivec2();
    let fraction = position - cell.as_vec2();
    let blend = fraction * fraction * (Vec2::splat(3.0) - 2.0 * fraction);
    let sample = |offset: IVec2| {
        let cell = cell + offset;
        random(
            (cell.x as u32).wrapping_mul(73_856_093)
                ^ (cell.y as u32).wrapping_mul(19_349_663)
                ^ 0x983c_b917,
        )
    };
    let lower = sample(IVec2::ZERO).lerp(sample(IVec2::X), blend.x);
    let upper = sample(IVec2::Y).lerp(sample(IVec2::ONE), blend.x);
    let value = ((lower.lerp(upper, blend.y) - 0.28) / 0.44).clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

/// The parcel's tree, if it has one. `empty` says whether a parcel is confirmed empty landscape;
/// the whole crown must be over such parcels. `height` is the terrain at Unity x/z.
fn placement(
    parcel: IVec2,
    empty: impl Fn(IVec2) -> bool,
    height: impl Fn(f32, f32) -> f32,
) -> Option<Placement> {
    if !empty(parcel) {
        return None;
    }
    let seed = (parcel.x as u32).wrapping_mul(73_856_093)
        ^ (parcel.y as u32).wrapping_mul(19_349_663)
        ^ 0x67ae;
    if random(seed) > 0.16 + 0.78 * grove_density(parcel) {
        return None;
    }
    let scale = 0.8 + random(seed ^ 0x521c) * 0.4;
    // Keep the usual parcel-center arrival point clear of trunks.
    let angle = random(seed ^ 0xa78c) * std::f32::consts::TAU;
    let offset = 2.5 + random(seed ^ 0x281f) * 2.5;
    let x = parcel.x as f32 * 16.0 + 8.0 + angle.cos() * offset;
    let z = parcel.y as f32 * 16.0 + 8.0 + angle.sin() * offset;
    let radius = CROWN_RADIUS * scale + 0.4;
    let min = ((Vec2::new(x, z) - radius) / 16.0).floor().as_ivec2();
    let max = ((Vec2::new(x, z) + radius) / 16.0).floor().as_ivec2();
    for py in min.y..=max.y {
        for px in min.x..=max.x {
            if !empty(IVec2::new(px, py)) {
                return None;
            }
        }
    }
    let y = height(x, z);
    let slope = [
        height(x - 1.5, z),
        height(x + 1.5, z),
        height(x, z - 1.5),
        height(x, z + 1.5),
    ]
    .into_iter()
    .map(|h| (h - y).abs())
    .fold(0.0, f32::max);
    if slope > MAX_SLOPE {
        return None;
    }
    Some(Placement {
        kind: (random(seed ^ 0xbb37) * KINDS as f32) as usize % KINDS,
        transform: Transform::from_xyz(x, y - SINK, -z)
            .with_rotation(Quat::from_rotation_y(
                random(seed ^ 0x9871) * std::f32::consts::TAU,
            ))
            .with_scale(Vec3::splat(scale)),
    })
}

/// Spawn and drop trees around the player's parcel, and keep them on the eased terrain.
fn update_trees(
    mut commands: Commands,
    surface: Res<TerrainSurface>,
    pointers: Res<ScenePointers>,
    assets: Res<TreeAssets>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    mut trees: Query<(Entity, &Tree, &mut Transform)>,
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
                let parcel = centre + offset;
                if let Some(tree) = placement(parcel, empty, |x, z| surface.height(x, z)) {
                    desired.insert(parcel, tree);
                }
            }
        }
        for (entity, tree, _) in &trees {
            if desired.remove(&tree.parcel).is_none() {
                commands.entity(entity).despawn();
            }
        }
        let (base, top) = trunk_axis();
        for (parcel, tree) in desired {
            if (parcel - centre).length_squared() > SPAWN_RANGE * SPAWN_RANGE {
                continue;
            }
            commands.spawn((
                Name::new("Landscape tree"),
                Tree { parcel },
                Mesh3d(assets.meshes[tree.kind].clone()),
                MeshMaterial3d(assets.material.clone()),
                tree.transform,
                assets.bounds[tree.kind],
                Solid::Capsule {
                    base,
                    top,
                    radius: TRUNK_RADIUS,
                },
            ));
        }
    }

    // parcels whose values reach a tree's parcel through bilinear sampling
    let resnap = |parcel: IVec2| {
        changes.relayout
            || changes.params
            || changes.eased.is_some_and(|(min, max)| {
                (min - 1).cmple(parcel).all() && (max + 1).cmpge(parcel).all()
            })
    };
    for (_, tree, mut transform) in &mut trees {
        if !resnap(tree.parcel) {
            continue;
        }
        let y = surface.height(transform.translation.x, -transform.translation.z) - SINK;
        if transform.translation.y != y {
            transform.translation.y = y;
        }
    }
}
