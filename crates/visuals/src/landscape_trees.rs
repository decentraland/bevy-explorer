use bevy::{
    asset::{embedded_asset, embedded_path},
    pbr::{ExtendedMaterial, MaterialExtension},
    platform::collections::HashMap,
    prelude::*,
    render::primitives::Aabb,
    render::render_resource::AsBindGroup,
    render::render_resource::ShaderRef,
};
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

use crate::{
    ground_visibility::GroundVisibility,
    tree_geometry::{random, tree_mesh, trunk_collision, CROWN_RADIUS, KINDS},
};

#[cfg(test)]
mod tests;

pub(super) struct LandscapeTreesPlugin;

impl Plugin for LandscapeTreesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "tree_wind.wgsl");
        embedded_asset!(app, "tree_lighting.wgsl");
        app.add_plugins(MaterialPlugin::<TreeMaterial>::default())
            .init_resource::<TreeState>()
            .init_resource::<TreeMode>()
            .add_systems(Startup, setup)
            .add_systems(
                Update,
                (refresh.after(crate::terrain_loading::load), update_lods).chain(),
            )
            .add_console_command::<TreesCommand, _>(trees_command);
    }
}

pub(super) type TreeMaterial = ExtendedMaterial<TreeWind>;

#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(super) struct TreeWind {
    #[uniform(100)]
    settings: Vec4,
    #[storage(101, read_only)]
    ambient: Handle<bevy::render::storage::ShaderStorageBuffer>,
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
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("tree_lighting.wgsl").display()
            )
            .into(),
        )
    }
}

#[derive(Resource)]
pub(super) struct TreeAssets {
    meshes: Vec<[Handle<Mesh>; 3]>,
    bounds: Vec<[Aabb; 3]>,
    pub material: Handle<TreeMaterial>,
    trunk: Mesh,
}

pub(super) fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TreeMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut bounds = Vec::new();
    let models = (0..KINDS)
        .map(|kind| {
            let models: [Mesh; 3] = std::array::from_fn(|lod| tree_mesh(kind, lod));
            bounds.push(crate::tree_geometry::tree_lod_bounds(&models, 0.4));
            models.map(|mesh| meshes.add(mesh))
        })
        .collect();
    let material = materials.add(ExtendedMaterial {
        base: StandardMaterial {
            base_color_texture: Some(images.add(crate::tree_leaves::image())),
            alpha_mode: AlphaMode::Mask(0.5),
            cull_mode: None,
            double_sided: true,
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        },
        extension: TreeWind {
            settings: Vec4::new(0.18, 0.018, 0.0, 0.0),
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
        },
    });
    commands.insert_resource(TreeAssets {
        meshes: models,
        bounds,
        material,
        trunk: trunk_collision(),
    });
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Resource, clap::ValueEnum)]
enum TreeMode {
    Off,
    #[default]
    Auto,
    High,
    Low,
}

#[derive(Component)]
pub(super) struct LandscapeTree {
    parcel: IVec2,
    kind: usize,
    lod: usize,
}

#[derive(Component)]
struct TreeColliders;

#[derive(Resource, Default)]
struct TreeState {
    key: Option<(u64, IVec2)>,
    collision_trees: usize,
    collision_signature: Vec<Placement>,
    collider_retry: bool,
    collider_builds: u64,
    lod_passes: u64,
    lod_instances: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    parcel: IVec2,
    kind: usize,
    transform: Transform,
}

fn empty(parcel: IVec2, field: &TerrainField, pointers: &ScenePointers) -> bool {
    if matches!(pointers.get(parcel), Some(PointerResult::Exists { .. })) {
        return false;
    }
    let pixel = parcel + IVec2::splat(field.size as i32 / 2);
    pixel.cmpge(IVec2::ZERO).all()
        && pixel.cmplt(IVec2::splat(field.size as i32)).all()
        && field.pixels[pixel.y as usize * field.size + pixel.x as usize] != 0
        && field.contains(parcel.x as f32 * 16.0 + 8.0, parcel.y as f32 * 16.0 + 8.0)
}

fn grove_density(parcel: IVec2) -> f32 {
    // A continuous, world-anchored 64 m field groups the existing per-parcel
    // candidates into groves and clearings. No extra instances or frame work.
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

fn placement(parcel: IVec2, field: &TerrainField, pointers: &ScenePointers) -> Option<Placement> {
    // Unknown parcels remain clear until the connected realm confirms they are empty.
    if !matches!(pointers.get(parcel), Some(PointerResult::Nothing))
        || !empty(parcel, field, pointers)
    {
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
    let min = IVec2::new(
        ((x - radius) / 16.0).floor() as i32,
        ((z - radius) / 16.0).floor() as i32,
    );
    let max = IVec2::new(
        ((x + radius) / 16.0).floor() as i32,
        ((z + radius) / 16.0).floor() as i32,
    );
    for py in min.y..=max.y {
        for px in min.x..=max.x {
            if !empty(IVec2::new(px, py), field, pointers) {
                return None;
            }
        }
    }
    let y = field.height(x, z);
    let slope = [
        field.height(x - 1.5, z),
        field.height(x + 1.5, z),
        field.height(x, z - 1.5),
        field.height(x, z + 1.5),
    ]
    .into_iter()
    .map(|h| (h - y).abs())
    .fold(0.0, f32::max);
    if slope > 0.85 {
        return None;
    }
    Some(Placement {
        parcel,
        kind: (random(seed ^ 0xbb37) * KINDS as f32) as usize % KINDS,
        transform: Transform::from_xyz(x, y - 0.06, -z)
            .with_rotation(Quat::from_rotation_y(
                random(seed ^ 0x9871) * std::f32::consts::TAU,
            ))
            .with_scale(Vec3::splat(scale)),
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn refresh(
    mut commands: Commands,
    mut state: ResMut<TreeState>,
    mode: Res<TreeMode>,
    terrain: Res<LandscapeTerrain>,
    visible: Res<GroundVisibility>,
    pointers: Res<ScenePointers>,
    assets: Res<TreeAssets>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    trees: Query<(Entity, &LandscapeTree)>,
    colliders: Query<Entity, With<TreeColliders>>,
) {
    let position = player
        .single()
        .ok()
        .map(GlobalTransform::translation)
        .filter(|p| p.is_finite() && p.abs().max_element() < 1_000_000.0);
    let center = position.map(vec3_to_parcel);
    let enabled =
        *mode != TreeMode::Off && visible.0 && terrain.field.is_some() && center.is_some();
    let key = enabled.then(|| (terrain.generation, center.unwrap()));
    let collider_present = colliders.iter().count();
    if state.key == key
        && !pointers.is_changed()
        && !mode.is_changed()
        && !visible.is_changed()
        && !state.collider_retry
        && collider_present == usize::from(!state.collision_signature.is_empty())
    {
        return;
    }
    let generation_changed = state.key.map(|k| k.0) != key.map(|k| k.0);
    state.key = key;
    let mut desired = HashMap::new();
    if let (Some((_, center)), Some(field)) = (key, terrain.field.as_ref()) {
        for y in -11i32..=11 {
            for x in -11i32..=11 {
                if x * x + y * y > 121 {
                    continue;
                }
                let parcel = center + IVec2::new(x, y);
                if let Some(tree) = placement(parcel, field, &pointers) {
                    desired.insert(parcel, tree);
                }
            }
        }
    }
    // Collision geometry is world-space and independent of rendering detail.
    // Compare actual placements, not the pointer resource's change tick: unrelated
    // scene responses and render-quality changes must not regenerate this BVH.
    let signature = collision_signature(desired.values().copied(), position);
    if state.collider_retry
        || state.collision_signature != signature
        || collider_present != usize::from(!signature.is_empty())
    {
        let mut collision_mesh: Option<Mesh> = None;
        for tree in &signature {
            let trunk = assets.trunk.clone().transformed_by(tree.transform);
            if let Some(mesh) = &mut collision_mesh {
                mesh.merge(&trunk).expect("identical trunk layouts");
            } else {
                collision_mesh = Some(trunk);
            }
        }
        let next = collision_mesh
            .as_ref()
            .map(SceneColliderData::from_terrain_mesh)
            .transpose();
        match next {
            Ok(collider) => {
                for entity in &colliders {
                    commands.entity(entity).despawn();
                }
                if let Some(collider) = collider {
                    commands.spawn((
                        Name::new("Landscape tree collisions"),
                        TreeColliders,
                        collider,
                        Transform::default(),
                    ));
                    state.collider_builds += 1;
                }
                state.collision_trees = signature.len();
                state.collision_signature = signature;
                state.collider_retry = false;
            }
            Err(error) => {
                state.collider_retry = true;
                error!("Tree collision generation failed: {error}");
            }
        }
    }
    for (entity, tree) in &trees {
        if generation_changed || !desired.contains_key(&tree.parcel) {
            commands.entity(entity).despawn();
        } else {
            desired.remove(&tree.parcel);
        }
    }
    for tree in desired.values() {
        commands.spawn((
            Name::new("Landscape tree"),
            LandscapeTree {
                parcel: tree.parcel,
                kind: tree.kind,
                lod: 2,
            },
            Mesh3d(assets.meshes[tree.kind][2].clone()),
            MeshMaterial3d(assets.material.clone()),
            tree.transform,
            Visibility::Inherited,
            assets.bounds[tree.kind][2],
            // VisibilityRange's 32-view table can exclude the player camera
            // when avatar previews are alive. Bound population by parcel radius
            // instead; ordinary frustum culling and distance LOD still apply.
        ));
    }
}

fn collision_signature(
    placements: impl Iterator<Item = Placement>,
    position: Option<Vec3>,
) -> Vec<Placement> {
    let mut signature: Vec<_> = placements
        .filter(|tree| {
            position.is_some_and(|p| (tree.transform.translation - p).xz().length() < 64.0)
        })
        .collect();
    signature.sort_unstable_by_key(|tree| (tree.parcel.x, tree.parcel.y));
    signature
}

// Retained as the reference for threshold-parity tests. Production avoids sqrt.
#[cfg(test)]
fn lod(distance: f32, previous: usize, mode: TreeMode) -> usize {
    match mode {
        TreeMode::High => 0,
        TreeMode::Low => 2,
        _ => match previous {
            0 if distance < 52.0 => 0,
            2 if distance > 92.0 => 2,
            _ if distance < 44.0 => 0,
            _ if distance > 100.0 => 2,
            _ => 1,
        },
    }
}

fn lod_squared(distance_squared: f32, previous: usize, mode: TreeMode) -> usize {
    // sqrt(1936.next_down()) rounds to exactly 44 in f32. Preserve the old
    // strict comparison at this one rounding boundary as well as at 44 itself.
    const NEAR: f32 = (44.0_f32 * 44.0).next_down();
    match mode {
        TreeMode::High => 0,
        TreeMode::Low => 2,
        _ => match previous {
            0 if distance_squared < 52.0 * 52.0 => 0,
            2 if distance_squared > 92.0 * 92.0 => 2,
            _ if distance_squared < NEAR => 0,
            _ if distance_squared > 100.0 * 100.0 => 2,
            _ => 1,
        },
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_lods(
    assets: Res<TreeAssets>,
    mode: Res<TreeMode>,
    mut state: ResMut<TreeState>,
    mut previous: Local<Option<(Vec3, TreeMode)>>,
    camera: Query<&GlobalTransform, With<PrimaryCamera>>,
    mut instances: ParamSet<(
        Query<
            (),
            (
                With<LandscapeTree>,
                Or<(Added<LandscapeTree>, Changed<Transform>)>,
            ),
        >,
        Query<(&Transform, &mut LandscapeTree, &mut Mesh3d, &mut Aabb)>,
    )>,
) {
    let Ok(camera) = camera.single() else {
        *previous = None;
        return;
    };
    let current = (camera.translation(), *mode);
    if *previous == Some(current) && instances.p0().is_empty() {
        return;
    }
    *previous = Some(current);
    state.lod_passes += 1;
    for (transform, mut tree, mut mesh, mut bounds) in &mut instances.p1() {
        state.lod_instances += 1;
        let next = lod_squared(
            transform.translation.distance_squared(current.0),
            tree.lod,
            *mode,
        );
        if next != tree.lod {
            tree.lod = next;
            mesh.0 = assets.meshes[tree.kind][next].clone();
            // Match the selected mesh before PostUpdate visibility/shadow
            // culling. Never retain a smaller bound from the previous LOD.
            *bounds = assets.bounds[tree.kind][next];
        }
    }
}

#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/trees")]
struct TreesCommand {
    #[arg(value_enum)]
    mode: Option<TreeMode>,
}

fn trees_command(
    mut input: ConsoleCommand<TreesCommand>,
    mut mode: ResMut<TreeMode>,
    state: Res<TreeState>,
    trees: Query<(
        Entity,
        &LandscapeTree,
        &ViewVisibility,
        &GlobalTransform,
        &InheritedVisibility,
    )>,
    cameras: Query<(Entity, &GlobalTransform, Has<PrimaryCamera>), With<Camera>>,
    meshes: Res<Assets<Mesh>>,
    assets: Res<TreeAssets>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Some(next) = command.mode {
            mode.set_if_neq(next);
        }
        let mut lods = [0; 3];
        let mut vertices = 0;
        for (_, tree, _, _, _) in &trees {
            lods[tree.lod] += 1;
            vertices += meshes
                .get(&assets.meshes[tree.kind][tree.lod])
                .map_or(0, Mesh::count_vertices);
        }
        let mut report = format!("landscape_trees: count={} visible={} vertices={vertices} lods={lods:?} colliders={} mode={:?}", trees.iter().count(), trees.iter().filter(|(_, _, v, _, _)| v.get()).count(), state.collision_trees, *mode);
        report.push_str(&format!(
            "\ncpu: lod_passes={} lod_instances={} collider_builds={}",
            state.lod_passes, state.lod_instances, state.collider_builds,
        ));
        let primary = cameras.iter().find(|(_, _, primary)| *primary);
        report.push_str(&format!(
            "\ncameras={} primary={primary:?}",
            cameras.iter().count()
        ));
        for (_, tree, _, transform, inherited) in trees.iter().take(8) {
            report.push_str(&format!(
                "\ntree: parcel=({}, {}) position={:?} kind={} lod={} inherited={}",
                tree.parcel.x,
                tree.parcel.y,
                transform.translation(),
                tree.kind,
                tree.lod,
                inherited.get()
            ));
        }
        input.reply_ok(report);
    }
}
