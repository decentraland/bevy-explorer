//! Empty-parcel terrain. Step targets are resolved in `ScenePointers`; this eases them, feeds the
//! eased values to the ground and grass vertex shader, and builds the player's ground collider from
//! the same values so what you see is what you stand on.

use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle, RenderAssetUsages},
    pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin},
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssets,
        render_resource::{
            AsBindGroup, Extent3d, Origin3d, ShaderRef, TexelCopyBufferLayout,
            TexelCopyTextureInfo, TextureAspect, TextureDimension, TextureFormat,
        },
        renderer::RenderQueue,
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};
use common::{
    sets::PostUpdateSets,
    structs::{CurrentRealm, EngineMovementControl, PrimaryUser},
    terrain::{
        height, texel_value, uv_size, PlayerTerrainHeight, TerrainChange, TerrainTargets,
        MAX_STEPS, ZERO,
    },
};
use dcl_component::SceneEntityId;
use ipfs::{ipfs_path::IpfsPath, EntityDefinition};
use scene_runner::{
    initialize_scene::{PointerResult, ScenePointers},
    update_world::mesh_collider::{ColliderId, GlobalGroundCollider, SceneColliderData},
    vec3_to_parcel,
};

pub(crate) const TERRAIN_TEXTURE: Handle<Image> =
    weak_handle!("540a82e9-5602-4123-9dc7-3d76e0aec0e5");

/// Eased change of a parcel's step value per second. One step is 1.8 m of height.
const EASE_STEPS_PER_SECOND: f32 = 1.0;
/// Side of the player's collider patch in metres, at 1 m spacing.
const COLLIDER_SIZE: i32 = 64;
/// The collider patch recentres on this grid (metres).
const COLLIDER_RECENTRE: f32 = 16.0;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct TerrainSet;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "terrain_vertex.wgsl");
        embedded_asset!(app, "terrain_params.wgsl");
        embedded_asset!(app, "terrain_flat.wgsl");
        embedded_asset!(app, "coast_profile.wgsl");

        app.init_resource::<TerrainSurface>()
            .init_resource::<PlayerTerrainHeight>()
            .init_resource::<TerrainTextureWrites>()
            .add_plugins(ExtractResourcePlugin::<TerrainTextureWrites>::default())
            .add_plugins(MaterialPlugin::<TerrainFlatMaterial>::default())
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                (
                    (sync, ease, (update_texture, player_height))
                        .chain()
                        .in_set(TerrainSet)
                        .before(PostUpdateSets::ColliderUpdate),
                    update_collider.in_set(PostUpdateSets::ColliderUpdate),
                    floor_player
                        .after(PostUpdateSets::PlayerUpdate)
                        .before(PostUpdateSets::CameraUpdate),
                ),
            );

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.add_systems(
                Render,
                write_texture
                    .in_set(RenderSet::PrepareResources)
                    .after(RenderSet::PrepareAssets),
            );
        }
    }
}

pub(crate) fn terrain_vertex_shader() -> ShaderRef {
    ShaderRef::Path(
        format!(
            "embedded://{}",
            embedded_path!("terrain_vertex.wgsl").display()
        )
        .into(),
    )
}

mod decl {
    // temporary for ShaderType macro, remove in future
    #![allow(dead_code)]

    use bevy::{
        math::{IVec2, Vec2, Vec4},
        render::render_resource::ShaderType,
    };

    #[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
    pub struct TerrainParams {
        pub min: IVec2,
        pub size: IVec2,
        pub max_steps: f32,
        pub uv_size: f32,
        /// ground and grass resolution follows parcel grass lod around this parcel (x, unity z)
        pub player_parcel: IVec2,
        /// the player's position within that parcel, 0 to 1 (x, unity z)
        pub player_offset: Vec2,
        /// coast bounds in unity x/z metres (min x, min z, max x, max z); the ground ends at the
        /// cliffs
        pub coast: Vec4,
    }

    /// Coast bounds that no point is beyond.
    pub const NO_COAST: Vec4 = Vec4::new(-1.0e7, -1.0e7, 1.0e7, 1.0e7);

    impl Default for TerrainParams {
        fn default() -> Self {
            Self {
                min: IVec2::ZERO,
                size: IVec2::ZERO,
                max_steps: 0.0,
                uv_size: 0.0,
                player_parcel: IVec2::ZERO,
                player_offset: Vec2::ZERO,
                coast: NO_COAST,
            }
        }
    }
}
pub use decl::*;

/// Flat ground colour (used when parcel grass is off), displaced like the grass.
pub type TerrainFlatMaterial = ExtendedMaterial<TerrainExtension>;

#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct TerrainExtension {
    #[texture(100, sample_type = "float", filterable = false, visibility(vertex))]
    pub terrain_steps: Handle<Image>,
    #[uniform(101)]
    pub terrain: TerrainParams,
}

impl Default for TerrainExtension {
    fn default() -> Self {
        Self {
            terrain_steps: TERRAIN_TEXTURE,
            terrain: TerrainParams::default(),
        }
    }
}

impl MaterialExtension for TerrainExtension {
    type Base = StandardMaterial;

    fn vertex_shader() -> ShaderRef {
        terrain_vertex_shader()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        terrain_vertex_shader()
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("terrain_flat.wgsl").display()
            )
            .into(),
        )
    }

    fn prepass_fragment_shader() -> ShaderRef {
        Self::fragment_shader()
    }
}

/// What changed in the surface this frame.
#[derive(Default, Clone, Debug)]
pub struct SurfaceChanges {
    /// The grid was reallocated; everything may differ.
    pub relayout: bool,
    /// Shader parameters (bounds, step count) changed.
    pub params: bool,
    /// Only the player's position within its parcel changed, which the shader's detail morph uses.
    pub player: bool,
    /// Parcels whose eased value changed.
    pub eased: Option<(IVec2, IVec2)>,
    /// Parcels whose target changed.
    pub targets: Option<(IVec2, IVec2)>,
}

/// Eased per-parcel step values over the terrain bounds.
#[derive(Resource)]
pub struct TerrainSurface {
    epoch: Option<u64>,
    enabled: bool,
    bounds: Option<(IVec2, IVec2)>,
    target: Vec<i8>,
    eased: Vec<f32>,
    active: Vec<usize>,
    is_active: Vec<bool>,
    max_steps_target: f32,
    max_steps: f32,
    uv_size: f32,
    player_parcel: IVec2,
    player_offset: Vec2,
    coast: Vec4,
    changes: SurfaceChanges,
}

impl Default for TerrainSurface {
    fn default() -> Self {
        Self {
            epoch: None,
            enabled: true,
            bounds: None,
            target: Vec::new(),
            eased: Vec::new(),
            active: Vec::new(),
            is_active: Vec::new(),
            max_steps_target: f32::from(MAX_STEPS),
            max_steps: f32::from(MAX_STEPS),
            uv_size: 1.0,
            player_parcel: IVec2::ZERO,
            player_offset: Vec2::ZERO,
            coast: NO_COAST,
            changes: SurfaceChanges::default(),
        }
    }
}

fn union(rect: Option<(IVec2, IVec2)>, min: IVec2, max: IVec2) -> Option<(IVec2, IVec2)> {
    Some(match rect {
        Some((a, b)) => (a.min(min), b.max(max)),
        None => (min, max),
    })
}

impl TerrainSurface {
    pub fn changes(&self) -> &SurfaceChanges {
        &self.changes
    }

    /// Coast bounds in unity x/z metres (min x, min z, max x, max z), if there is a coast.
    pub fn coast(&self) -> Option<Vec4> {
        (self.coast != NO_COAST).then_some(self.coast)
    }

    fn width(&self) -> i32 {
        self.bounds.map_or(0, |(min, max)| max.x - min.x + 1)
    }

    fn index(&self, parcel: IVec2) -> Option<usize> {
        let (min, max) = self.bounds?;
        if parcel.cmplt(min).any() || parcel.cmpgt(max).any() {
            return None;
        }
        let offset = parcel - min;
        Some((offset.y * self.width() + offset.x) as usize)
    }

    pub fn params(&self) -> TerrainParams {
        match self.bounds {
            Some((min, max)) => TerrainParams {
                min,
                size: max - min + 1,
                max_steps: self.max_steps,
                uv_size: self.uv_size,
                player_parcel: self.player_parcel,
                player_offset: self.player_offset,
                coast: self.coast,
            },
            None => TerrainParams {
                min: IVec2::ZERO,
                size: IVec2::ONE,
                max_steps: 0.0,
                uv_size: 1.0,
                player_parcel: self.player_parcel,
                player_offset: self.player_offset,
                coast: self.coast,
            },
        }
    }

    fn texel(&self, parcel: IVec2) -> f32 {
        self.index(parcel)
            .map_or(0.0, |index| texel_value(self.eased[index], self.max_steps))
    }

    /// Current (eased) height at Unity x/z.
    pub fn height(&self, x: f32, unity_z: f32) -> f32 {
        height(x, unity_z, self.max_steps, self.uv_size, |parcel| {
            self.texel(parcel)
        })
    }

    /// Current (eased) height below a Bevy position.
    pub fn height_at(&self, position: Vec3) -> f32 {
        self.height(position.x, -position.z)
    }

    /// Height of the 1 m triangulated surface the collider and nearby ground use: the height
    /// function at whole metres, each cell split along Unity's (x0, z0)-(x1, z1) diagonal.
    pub fn cell_height_at(&self, position: Vec3) -> f32 {
        let p = Vec2::new(position.x, -position.z);
        let base = p.floor();
        let (u, v) = (p.x - base.x, p.y - base.y);
        let corner = |x: f32, z: f32| self.height(base.x + x, base.y + z);
        let h00 = corner(0.0, 0.0);
        let h11 = corner(1.0, 1.0);
        if u >= v {
            let h10 = corner(1.0, 0.0);
            h00 + u * (h10 - h00) + v * (h11 - h10)
        } else {
            let h01 = corner(0.0, 1.0);
            h00 + v * (h01 - h00) + u * (h11 - h01)
        }
    }

    /// True if the landscape is shown and the parcel is inside the terrain bounds.
    pub fn contains(&self, parcel: IVec2) -> bool {
        self.enabled && self.index(parcel).is_some()
    }

    /// True if the parcel will be flat at height 0 once eased: no neighbouring target rises.
    pub fn is_flat(&self, parcel: IVec2) -> bool {
        (-1..=1).all(|y| {
            (-1..=1).all(|x| {
                self.index(parcel + IVec2::new(x, y))
                    .is_none_or(|index| self.target[index] <= 0)
            })
        })
    }

    fn activate(&mut self, index: usize) {
        if !self.is_active[index] && self.eased[index] != f32::from(self.target[index]) {
            self.is_active[index] = true;
            self.active.push(index);
        }
    }

    fn apply(&mut self, targets: &TerrainTargets, change: Option<TerrainChange>, enabled: bool) {
        let snap = self.epoch != Some(targets.epoch()) || self.enabled != enabled;
        let target = |parcel| {
            if enabled {
                targets.steps(parcel)
            } else {
                ZERO
            }
        };
        if snap || self.bounds != targets.bounds() || change == Some(TerrainChange::Full) {
            let old_bounds = self.bounds;
            let old_width = self.width();
            let old_eased = std::mem::take(&mut self.eased);
            let old_index = |parcel: IVec2| {
                let (min, max) = old_bounds?;
                if parcel.cmplt(min).any() || parcel.cmpgt(max).any() {
                    return None;
                }
                let offset = parcel - min;
                Some((offset.y * old_width + offset.x) as usize)
            };
            self.bounds = targets.bounds();
            self.uv_size = self.bounds.map_or(1.0, uv_size);
            self.active.clear();
            self.target.clear();
            if let Some((min, max)) = self.bounds {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        let parcel = IVec2::new(x, y);
                        let steps = target(parcel);
                        self.target.push(steps);
                        self.eased.push(if snap {
                            f32::from(steps)
                        } else {
                            old_index(parcel).map_or(f32::from(ZERO), |index| old_eased[index])
                        });
                    }
                }
            }
            self.is_active = vec![false; self.target.len()];
            for index in 0..self.target.len() {
                self.activate(index);
            }
            self.changes.relayout = true;
            self.changes.params = true;
            self.changes.targets = self.bounds;
            self.changes.eased = self.bounds;
        } else if let Some(TerrainChange::Rect(min, max)) = change {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let parcel = IVec2::new(x, y);
                    let index = self.index(parcel).unwrap();
                    let steps = target(parcel);
                    if self.target[index] != steps {
                        self.target[index] = steps;
                        self.activate(index);
                        self.changes.targets = union(self.changes.targets, parcel, parcel);
                    }
                }
            }
        }
        self.max_steps_target = f32::from(targets.max_steps());
        if snap {
            self.max_steps = self.max_steps_target;
        }
        self.epoch = Some(targets.epoch());
        self.enabled = enabled;
    }

    /// Move eased values toward their targets. Occupied (`ZERO`) texels lie far below the floor,
    /// so leaving `ZERO` jumps straight to step 0: every neighbour of an occupied parcel is at
    /// step 0 or below, so no height changes at the jump.
    fn ease(&mut self, dt: f32) {
        let step = dt * EASE_STEPS_PER_SECOND;
        let mut changed = self.changes.eased;
        let Self {
            bounds,
            target,
            eased,
            active,
            is_active,
            ..
        } = self;
        let width = bounds.map_or(0, |(min, max)| max.x - min.x + 1);
        active.retain(|&index| {
            let goal = f32::from(target[index]);
            let value = &mut eased[index];
            let before = *value;
            if target[index] == ZERO {
                *value = if *value - step <= 0.0 {
                    goal
                } else {
                    *value - step
                };
            } else {
                let from = value.max(0.0);
                *value = if from < goal {
                    (from + step).min(goal)
                } else {
                    (from - step).max(goal)
                };
            }
            if *value != before {
                let parcel =
                    bounds.unwrap().0 + IVec2::new(index as i32 % width, index as i32 / width);
                changed = union(changed, parcel, parcel);
            }
            let settled = *value == goal;
            if settled {
                is_active[index] = false;
            }
            !settled
        });
        self.changes.eased = changed;

        if self.max_steps != self.max_steps_target {
            self.max_steps = if self.max_steps < self.max_steps_target {
                (self.max_steps + step).min(self.max_steps_target)
            } else {
                (self.max_steps - step).max(self.max_steps_target)
            };
            self.changes.params = true;
        }
    }

    fn rect_bytes(&self, min: IVec2, max: IVec2) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(((max - min + 1).element_product() * 4) as usize);
        for y in min.y..=max.y {
            for x in min.x..=max.x {
                let index = self.index(IVec2::new(x, y)).unwrap();
                bytes.extend_from_slice(&self.eased[index].to_le_bytes());
            }
        }
        bytes
    }

    fn image(&self) -> Image {
        let (size, bytes) = match self.bounds {
            Some((min, max)) => (max - min + 1, self.rect_bytes(min, max)),
            None => (IVec2::ONE, f32::from(ZERO).to_le_bytes().to_vec()),
        };
        Image::new(
            Extent3d {
                width: size.x as u32,
                height: size.y as u32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            bytes,
            TextureFormat::R32Float,
            RenderAssetUsages::RENDER_WORLD,
        )
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>, surface: Res<TerrainSurface>) {
    images.insert(TERRAIN_TEXTURE.id(), surface.image());
    commands.spawn((
        Name::new("Terrain collider"),
        TerrainCollider::default(),
        SceneColliderData::default(),
        GlobalGroundCollider,
        Transform::default(),
    ));
}

/// unity-explorer skips the landscape for a single-scene realm whose scene opts out. None until
/// that scene's definition has loaded.
fn terrain_enabled(realm: &CurrentRealm, definitions: &Assets<EntityDefinition>) -> Option<bool> {
    let Some([urn]) = realm.config.scenes_urn.as_deref() else {
        return Some(true);
    };
    let Some(hash) = IpfsPath::new_from_urn::<EntityDefinition>(&urn.replace('?', "?=&"))
        .ok()
        .and_then(|path| path.context_free_hash().ok().flatten())
    else {
        return Some(true);
    };
    let (_, definition) = definitions
        .iter()
        .find(|(_, definition)| definition.id == hash)?;
    Some(
        definition
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("landscapeTerrain"))
            .and_then(|value| value.as_bool())
            != Some(false),
    )
}

fn sync(
    mut pointers: ResMut<ScenePointers>,
    realm: Option<Res<CurrentRealm>>,
    definitions: Option<Res<Assets<EntityDefinition>>>,
    // the realm's answer, kept once its scene's definition is found: the definition is dropped
    // when the scene unloads
    mut enabled: Local<Option<bool>>,
    mut surface: ResMut<TerrainSurface>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
) {
    surface.changes = SurfaceChanges::default();
    if let Ok(player) = player.single() {
        let translation = player.translation();
        let parcel = vec3_to_parcel(translation);
        if parcel != surface.player_parcel {
            surface.player_parcel = parcel;
            surface.changes.params = true;
        }
        let offset = Vec2::new(translation.x, -translation.z) / 16.0 - parcel.as_vec2();
        if offset.is_finite() && offset != surface.player_offset {
            surface.player_offset = offset;
            surface.changes.player = true;
        }
    }
    let realm_changed = realm.as_ref().is_some_and(|realm| realm.is_changed());
    if realm_changed {
        *enabled = None;
    }
    if enabled.is_none()
        && (realm_changed || definitions.as_ref().is_some_and(|defs| defs.is_changed()))
    {
        if let (Some(realm), Some(definitions)) = (realm, definitions) {
            *enabled = terrain_enabled(&realm, &definitions);
        }
    }
    // only take the pointers mutably when there is work, so scene-layout change detection on
    // `ScenePointers` stays quiet
    let change = if pointers.terrain().needs_resolve() {
        pointers.resolve_terrain()
    } else {
        None
    };
    let enabled = enabled.unwrap_or(true);
    surface.apply(pointers.terrain(), change, enabled);
    let coast = pointers
        .coast_bounds()
        .filter(|_| enabled)
        .map_or(NO_COAST, |(min, max)| {
            let (min, max) = ((min * 16).as_vec2(), ((max + 1) * 16).as_vec2());
            Vec4::new(min.x, min.y, max.x, max.y)
        });
    if coast != surface.coast {
        surface.coast = coast;
        surface.changes.params = true;
    }
}

fn ease(time: Res<Time>, mut surface: ResMut<TerrainSurface>) {
    surface.ease(time.delta_secs());
}

fn player_height(
    surface: Res<TerrainSurface>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    mut height: ResMut<PlayerTerrainHeight>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let position = player.translation();
    if position.is_finite() {
        height.0 = surface.height_at(position);
    }
}

/// Texel updates for the step texture, applied in the render world without re-uploading it.
#[derive(Resource, Default, Clone, ExtractResource)]
struct TerrainTextureWrites {
    size: IVec2,
    writes: Vec<(IVec2, IVec2, Vec<u8>)>,
}

fn update_texture(
    surface: Res<TerrainSurface>,
    mut images: ResMut<Assets<Image>>,
    mut writes: ResMut<TerrainTextureWrites>,
) {
    let changes = surface.changes();
    if changes.relayout {
        images.insert(TERRAIN_TEXTURE.id(), surface.image());
        if !writes.writes.is_empty() {
            writes.writes.clear();
        }
    } else if let Some((min, max)) = changes.eased {
        let (bounds_min, bounds_max) = surface.bounds.unwrap();
        *writes = TerrainTextureWrites {
            size: bounds_max - bounds_min + 1,
            writes: vec![(
                min - bounds_min,
                max - min + 1,
                surface.rect_bytes(min, max),
            )],
        };
    } else if !writes.writes.is_empty() {
        writes.writes.clear();
    }
}

fn write_texture(
    mut writes: ResMut<TerrainTextureWrites>,
    images: Res<RenderAssets<GpuImage>>,
    queue: Res<RenderQueue>,
) {
    let pending = std::mem::take(&mut writes.writes);
    let Some(image) = images.get(TERRAIN_TEXTURE.id()) else {
        return;
    };
    // a write for a newer layout than the uploaded texture is already in the new texture's data
    if image.size.width != writes.size.x as u32 || image.size.height != writes.size.y as u32 {
        return;
    }
    for (origin, size, bytes) in pending {
        queue.write_texture(
            TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: origin.x as u32,
                    y: origin.y as u32,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &bytes,
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.x as u32 * 4),
                rows_per_image: None,
            },
            Extent3d {
                width: size.x as u32,
                height: size.y as u32,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[derive(Component, Default)]
pub(crate) struct TerrainCollider {
    /// Bevy x/z of the patch centre in metres.
    centre: Option<IVec2>,
}

impl TerrainCollider {
    /// Bevy x/z bounds of the patch in metres.
    pub(crate) fn patch(&self) -> Option<(Vec2, Vec2)> {
        let half = Vec2::splat((COLLIDER_SIZE / 2) as f32);
        self.centre
            .map(|centre| (centre.as_vec2() - half, centre.as_vec2() + half))
    }
}

fn collider_id() -> ColliderId {
    ColliderId::new(SceneEntityId::ROOT, Some("terrain".into()), 0)
}

/// Heights for a `COLLIDER_SIZE` patch at 1 m spacing, rows along +z.
fn collider_heights(surface: &TerrainSurface, centre: IVec2) -> Vec<f32> {
    let half = COLLIDER_SIZE / 2;
    (0..=COLLIDER_SIZE)
        .flat_map(|row| {
            (0..=COLLIDER_SIZE).map(move |col| {
                let x = (centre.x - half + col) as f32;
                let z = (centre.y - half + row) as f32;
                surface.height(x, -z)
            })
        })
        .collect()
}

/// Rebuild the player's heightfield patch when it recentres or the heights under it change.
pub(crate) fn update_collider(
    surface: Res<TerrainSurface>,
    player: Query<&GlobalTransform, With<PrimaryUser>>,
    collider: Single<(&mut TerrainCollider, &mut SceneColliderData)>,
) {
    let Ok(player) = player.single() else {
        return;
    };
    let position = player.translation();
    if !position.is_finite() {
        return;
    }
    let (mut state, mut data) = collider.into_inner();
    let centre = match state.centre {
        Some(centre)
            if (position.xz() - centre.as_vec2()).abs().max_element() < COLLIDER_RECENTRE =>
        {
            centre
        }
        _ => ((position.xz() / COLLIDER_RECENTRE).round() * COLLIDER_RECENTRE).as_ivec2(),
    };
    let changes = surface.changes();
    let half = COLLIDER_SIZE / 2;
    // parcels (x, unity z) whose values reach the patch through bilinear sampling
    let covered = (
        IVec2::new(
            (centre.x - half).div_euclid(16) - 1,
            (-centre.y - half).div_euclid(16) - 1,
        ),
        IVec2::new(
            (centre.x + half).div_euclid(16) + 1,
            (-centre.y + half).div_euclid(16) + 1,
        ),
    );
    let affected = changes.relayout
        || changes.params
        || changes
            .eased
            .is_some_and(|(min, max)| min.cmple(covered.1).all() && max.cmpge(covered.0).all());
    if state.centre == Some(centre) && !affected {
        return;
    }
    state.centre = Some(centre);
    let side = (COLLIDER_SIZE + 1) as usize;
    data.set_ground_heightfield(
        &collider_id(),
        side,
        side,
        &collider_heights(&surface, centre),
        Vec2::splat(COLLIDER_SIZE as f32),
        Vec3::new(centre.x as f32, 0.0, centre.y as f32),
    );
}

/// Keep the player on or above the terrain of an empty parcel after movement each tick, as
/// movement already floors at zero. The avatar then rides rising ground up instead of fighting
/// it through collision; the heightfield collider stays for scene raycasts.
fn floor_player(
    surface: Res<TerrainSurface>,
    pointers: Res<ScenePointers>,
    control: Res<EngineMovementControl>,
    mut player: Query<&mut Transform, With<PrimaryUser>>,
) {
    if !control.suppress_avatar_physics.is_empty() || !control.suppress_clipping.is_empty() {
        return;
    }
    let Ok(mut transform) = player.single_mut() else {
        return;
    };
    let position = transform.translation;
    if !position.is_finite()
        || !matches!(
            pointers.get(vec3_to_parcel(position)),
            Some(PointerResult::Nothing)
        )
    {
        return;
    }
    let height = surface.cell_height_at(position);
    if position.y < height {
        transform.translation.y = height;
    }
}

struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    stitches: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl MeshBuilder {
    fn new() -> Self {
        Self {
            positions: Vec::new(),
            stitches: Vec::new(),
            indices: Vec::new(),
        }
    }

    /// A square of `segments`² cells from `start` (Unity x/z), `step` metres apart. Cells are
    /// split along unity-explorer's collider diagonal, (x0, z0)-(x1, z1) in Unity space.
    /// `stitch(x, z)` gives, per axis, the cell size of a coarser grid met by a vertex's edge
    /// along that axis; the shader places such vertices on the coarser grid's edge.
    fn grid(&mut self, start: Vec2, step: f32, segments: u32, stitch: impl Fn(u32, u32) -> Vec2) {
        self.grid_cells(start, step, segments, stitch, |_, _| true);
    }

    /// `grid`, emitting only the cells for which `keep(x, z)` holds.
    fn grid_cells(
        &mut self,
        start: Vec2,
        step: f32,
        segments: u32,
        stitch: impl Fn(u32, u32) -> Vec2,
        keep: impl Fn(u32, u32) -> bool,
    ) {
        let base = self.positions.len() as u32;
        for z in 0..=segments {
            for x in 0..=segments {
                let position = start + Vec2::new(x as f32, z as f32) * step;
                self.positions.push([position.x, 0.0, -position.y]);
                self.stitches.push(stitch(x, z).to_array());
            }
        }
        let row = segments + 1;
        for z in 0..segments {
            for x in (0..segments).filter(|x| keep(*x, z)) {
                let a = base + z * row + x;
                self.indices
                    .extend_from_slice(&[a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            }
        }
    }

    fn build(self) -> Mesh {
        let normals = vec![[0.0, 1.0, 0.0]; self.positions.len()];
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, self.stitches)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Parcel-sized grid centred on the origin.
pub(crate) fn parcel_mesh(segments: u32) -> Mesh {
    let mut builder = MeshBuilder::new();
    builder.grid(
        Vec2::splat(-8.0),
        16.0 / segments as f32,
        segments,
        |_, _| Vec2::ZERO,
    );
    builder.build()
}

/// Parcel offsets from the player's parcel that the ground covers with per-parcel grids at the
/// parcel grass resolution: every parcel that can hold grass (it is dropped beyond 150 parcels²).
pub(crate) const GROUND_NEAR_MIN: i32 = -13;
pub(crate) const GROUND_NEAR_MAX: i32 = 12;
/// Cell size of the first rings around the per-parcel grids; each further ring doubles it.
const RING_CELL: f32 = 16.0;
/// Rings at `RING_CELL`, which follow the terrain: their cells move with the player in whole cells,
/// so they sit on the same points. Coarser rings beyond would slide over it, so the ground there is
/// flat (terrain_vertex.wgsl FLAT_REACH).
const TERRAIN_RINGS: u32 = 2;

/// Flat ground relative to the player's parcel corner (Unity x/z). Parcels near the player get
/// `segments(offset)` cells a side, matching the parcel grass over them, so both share vertices.
/// Beyond, `rings` square rings each double the extent, with cells doubling after the first
/// `TERRAIN_RINGS`.
pub(crate) fn ground_mesh(segments: impl Fn(IVec2) -> u32, rings: u32) -> Mesh {
    let mut builder = MeshBuilder::new();
    let (min, max) = (GROUND_NEAR_MIN, GROUND_NEAR_MAX);
    for j in min..=max {
        for i in min..=max {
            let count = segments(IVec2::new(i, j));
            // the outer edge meets the first ring
            let stitch = move |x: u32, z: u32| {
                Vec2::new(
                    if (j == min && z == 0) || (j == max && z == count) {
                        RING_CELL
                    } else {
                        0.0
                    },
                    if (i == min && x == 0) || (i == max && x == count) {
                        RING_CELL
                    } else {
                        0.0
                    },
                )
            };
            builder.grid(
                IVec2::new(i, j).as_vec2() * 16.0,
                16.0 / count as f32,
                count,
                stitch,
            );
        }
    }
    let mut inner = (-min * 16) as f32;
    let mut cell = RING_CELL;
    for ring in 0..rings {
        let outer = inner * 2.0;
        let count = (outer * 2.0 / cell) as u32;
        // the next ring's cell, met on this ring's outer edge
        let next = if ring + 1 < TERRAIN_RINGS {
            cell
        } else {
            cell * 2.0
        };
        let stitch = move |x: u32, z: u32| {
            Vec2::new(
                if z == 0 || z == count { next } else { 0.0 },
                if x == 0 || x == count { next } else { 0.0 },
            )
        };
        let keep = move |x: u32, z: u32| {
            let corner = Vec2::new(x as f32, z as f32) * cell - outer;
            corner.cmplt(Vec2::splat(-inner)).any()
                || (corner + cell).cmpgt(Vec2::splat(inner)).any()
        };
        builder.grid_cells(Vec2::splat(-outer), cell, count, stitch, keep);
        inner = outer;
        cell = next;
    }
    builder.build()
}

/// Half-size of `ground_mesh` in metres.
pub(crate) fn ground_extent(rings: u32) -> f32 {
    (-GROUND_NEAR_MIN * 16) as f32 * (1 << rings) as f32
}

/// Ground placement: the corner of the player's parcel.
pub(crate) fn ground_origin(player: Vec3) -> Vec3 {
    let parcel = vec3_to_parcel(player);
    Vec3::new(parcel.x as f32 * 16.0, 0.0, -(parcel.y as f32) * 16.0)
}

#[cfg(test)]
mod tests;
