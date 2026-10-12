use std::marker::PhantomData;

use bevy::{
    asset::{weak_handle, RenderAssetUsages},
    pbr::{ExtendedMaterial, MaterialExtension},
    platform::collections::{hash_map::Entry, HashMap},
    prelude::*,
    render::{
        mesh::MeshTag,
        render_resource::{AsBindGroup, Face, ShaderDefVal, ShaderRef},
        storage::ShaderStorageBuffer,
    },
};
use boimp::bake::{
    ImposterBakeMaterial, ImposterBakeMaterialExtension, ImposterBakeMaterialPlugin,
};
use common::{
    structs::{PreviewMode, ShowOutOfBounds},
    util::InvertedScaleExt,
};

pub type SceneMaterial = ExtendedMaterial<SceneBound>;

pub const SCENE_MATERIAL_SHOW_OUTSIDE_BOUNDS_MESH_TAG: u32 = 0x01000000;
pub const SCENE_MATERIAL_NO_DITHERING_MESH_TAG: u32 = 0x02000000;
pub const SCENE_MATERIAL_CONE_ONLY_DITHER_MESH_TAG: u32 = 0x04000000;
// cel-shade this mesh (avatars) instead of standard PBR — see toon.wgsl
pub const SCENE_MATERIAL_TOON_MESH_TAG: u32 = 0x08000000;
pub const SCENE_MATERIAL_OUTLINE_MESH_TAGS: u32 = 0xF0000000;
pub const SCENE_MATERIAL_OUTLINE_BLACK_MESH_TAG: u32 = 0x10000000;
pub const SCENE_MATERIAL_OUTLINE_RED_MESH_TAG: u32 = 0x20000000;
pub const SCENE_MATERIAL_OUTLINE_GREEN_MESH_TAG: u32 = 0x40000000;
pub const SCENE_MATERIAL_OUTLINE_BLUE_MESH_TAG: u32 = 0x80000000;

pub trait SceneMaterialExt {
    fn new_unbounded(mat: StandardMaterial) -> Self
    where
        Self: Sized;
}

impl SceneMaterialExt for SceneMaterial {
    fn new_unbounded(mat: StandardMaterial) -> Self
    where
        Self: Sized,
    {
        Self {
            base: mat,
            extension: SceneBound::new_unbounded(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SceneBoundKey {
    inverted_scale: bool,
}

impl From<&SceneBound> for SceneBoundKey {
    fn from(value: &SceneBound) -> Self {
        Self {
            inverted_scale: value.inverted_scale,
        }
    }
}

#[derive(Asset, TypePath, Clone, AsBindGroup)]
#[bind_group_data(SceneBoundKey)]
pub struct SceneBound {
    #[uniform(100)]
    pub data: SceneBoundData,
    #[storage(101, read_only, visibility(fragment))]
    pub cells: Handle<ShaderStorageBuffer>,
    /// Meshes that have 1 or 3 negative scale axis require special handling
    /// of the face culling
    pub inverted_scale: bool,
}

impl SceneBound {
    pub fn new(bounds: &SceneBounds, distance: f32) -> Self {
        Self {
            data: SceneBoundData {
                distance,
                ..bounds.data
            },
            cells: bounds.cells.clone(),
            inverted_scale: false,
        }
    }

    pub fn new_unbounded() -> Self {
        Self::new(&SceneBounds::unbounded(), 0.0)
    }
}

pub const SCENE_BOUNDED: u32 = 1;
pub const SCENE_FULL_RECT: u32 = 2;
/// scenes with a larger bounding box fall back to the bounding box itself
const MAX_SCENE_CELLS: u64 = 1 << 20;
/// empty cell table for unbounded and full-rect scenes, which never read it
pub const EMPTY_SCENE_CELLS: Handle<ShaderStorageBuffer> =
    weak_handle!("7f375a13-2034-4519-b4e9-c570a0fb345b");

/// a scene's bounds in the form the scene-bound shaders read (see scene_bounds.wgsl). built once
/// per scene; the cell table is shared by all the scene's materials.
#[derive(Clone, Debug)]
pub struct SceneBounds {
    data: SceneBoundData,
    cells: Handle<ShaderStorageBuffer>,
}

impl Default for SceneBounds {
    fn default() -> Self {
        Self::unbounded()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct SceneBoundsKey {
    origin: IVec2,
    size: UVec2,
    rect_height: u32,
    flags: u32,
    cells: AssetId<ShaderStorageBuffer>,
}

impl SceneBounds {
    pub fn unbounded() -> Self {
        Self {
            data: SceneBoundData::default(),
            cells: EMPTY_SCENE_CELLS,
        }
    }

    /// a single rectangle, checked without the cell table
    pub fn rect(region: &BoundRegion) -> Self {
        let (min, max) = region.cells();
        Self::full_rect(min, max, region.height)
    }

    fn full_rect(min: IVec2, max: IVec2, height: f32) -> Self {
        Self {
            data: SceneBoundData {
                origin: min,
                size: (max - min + IVec2::ONE).as_uvec2(),
                rect_height: height,
                flags: SCENE_BOUNDED | SCENE_FULL_RECT,
                distance: 0.0,
            },
            cells: EMPTY_SCENE_CELLS,
        }
    }

    /// no regions = unbounded. without `buffers` (or for huge extents) an irregular scene falls
    /// back to its bounding box.
    pub fn new(regions: &[BoundRegion], buffers: Option<&mut Assets<ShaderStorageBuffer>>) -> Self {
        let [first, rest @ ..] = regions else {
            return Self::unbounded();
        };
        if rest.is_empty() {
            return Self::rect(first);
        }

        let (min, max) = regions
            .iter()
            .map(BoundRegion::cells)
            .fold((IVec2::MAX, IVec2::MIN), |(min, max), (rmin, rmax)| {
                (min.min(rmin), max.max(rmax))
            });
        let size = (max - min + IVec2::ONE).as_uvec2();
        let max_height = regions.iter().fold(0.0f32, |h, r| h.max(r.height));
        let cell_count = size.x as u64 * size.y as u64;
        let Some(buffers) = buffers.filter(|_| cell_count <= MAX_SCENE_CELLS) else {
            if cell_count > MAX_SCENE_CELLS {
                warn!("scene bounds {size} too large, using the bounding box");
            }
            return Self::full_rect(min, max, max_height);
        };

        // height table indexed from 1; 0 marks cells outside the scene
        let mut heights: Vec<f32> = Vec::new();
        let mut cells = vec![0u16; cell_count as usize];
        for region in regions {
            let index = match heights.iter().position(|h| *h == region.height) {
                Some(index) => index,
                None => {
                    heights.push(region.height);
                    heights.len() - 1
                }
            };
            let (rmin, rmax) = region.cells();
            for z in rmin.y..=rmax.y {
                for x in rmin.x..=rmax.x {
                    let local = (IVec2::new(x, z) - min).as_uvec2();
                    cells[(local.y * size.x + local.x) as usize] = index as u16 + 1;
                }
            }
        }

        if heights.len() == 1 && cells.iter().all(|c| *c != 0) {
            return Self::full_rect(min, max, heights[0]);
        }

        let words = std::iter::once(heights.len() as u32)
            .chain(heights.iter().map(|h| h.to_bits()))
            .chain(
                cells
                    .chunks(2)
                    .map(|pair| pair[0] as u32 | (pair.get(1).copied().unwrap_or(0) as u32) << 16),
            );
        let bytes = words.flat_map(u32::to_le_bytes).collect::<Vec<_>>();
        Self {
            data: SceneBoundData {
                origin: min,
                size,
                rect_height: max_height,
                flags: SCENE_BOUNDED,
                distance: 0.0,
            },
            cells: buffers.add(ShaderStorageBuffer::new(
                &bytes,
                RenderAssetUsages::RENDER_WORLD,
            )),
        }
    }

    /// identifies materials built from equal bounds
    pub fn key(&self) -> SceneBoundsKey {
        SceneBoundsKey {
            origin: self.data.origin,
            size: self.data.size,
            rect_height: self.data.rect_height.to_bits(),
            flags: self.data.flags,
            cells: self.cells.id(),
        }
    }
}

mod decl {
    // temporary for ShaderType macro, remove in future
    #![allow(dead_code)]

    use bevy::{
        math::{IVec2, UVec2},
        render::render_resource::ShaderType,
    };
    #[derive(ShaderType, Clone, Copy, Debug, Default)]
    pub struct BoundRegion {
        pub min: u32, // 2x i16
        pub max: u32, // 2x i16
        pub height: f32,
        pub parcel_count: u32,
    }

    /// see scene_bounds.wgsl
    #[derive(ShaderType, Clone, Copy, Debug, Default)]
    pub struct SceneBoundData {
        pub origin: IVec2,
        pub size: UVec2,
        pub rect_height: f32,
        pub flags: u32,
        pub distance: f32,
    }
}
pub use decl::*;

impl BoundRegion {
    pub fn new(min: IVec2, max: IVec2, parcel_count: u32) -> Self {
        Self {
            min: ((min.x as i16 as u16 as u32) << 16) | (-(max.y + 1) as i16 as u16 as u32),
            max: (((max.x + 1) as i16 as u16 as u32) << 16) | (-min.y as i16 as u16 as u32),
            height: f32::log2(parcel_count as f32 + 1.0) * 20.0,
            parcel_count,
        }
    }
}

impl BoundRegion {
    fn unpack_parcel_coords(input: u32) -> IVec2 {
        let x = ((input >> 16) & 0xFFFF) as i32;
        let y = (input & 0xFFFF) as i32;
        IVec2::new(
            if (x & 0x8000) != 0 { x - 0x10000 } else { x },
            if (y & 0x8000) != 0 { y - 0x10000 } else { y },
        )
    }

    pub fn parcel_min(&self) -> IVec2 {
        IVec2::new(
            Self::unpack_parcel_coords(self.min).x,
            -Self::unpack_parcel_coords(self.max).y,
        )
    }

    /// inclusive min/max parcel cells in world orientation (world xz / 16)
    pub fn cells(&self) -> (IVec2, IVec2) {
        let (min, max) = (self.parcel_min(), self.parcel_max());
        (IVec2::new(min.x, -max.y - 1), IVec2::new(max.x, -min.y - 1))
    }

    pub fn parcel_max(&self) -> IVec2 {
        IVec2::new(
            Self::unpack_parcel_coords(self.max).x - 1,
            -Self::unpack_parcel_coords(self.min).y - 1,
        )
    }

    pub fn world_min(&self) -> Vec3 {
        let coords = Self::unpack_parcel_coords(self.min).as_vec2() * 16.0;
        Vec3::new(coords.x, 0.0, coords.y)
    }

    pub fn world_max(&self) -> Vec3 {
        let coords = Self::unpack_parcel_coords(self.max).as_vec2() * 16.0;
        Vec3::new(coords.x, self.height, coords.y)
    }

    pub fn world_size(&self) -> Vec3 {
        self.world_max() - self.world_min()
    }

    pub fn world_midpoint(&self) -> Vec3 {
        (self.world_max() + self.world_min()) * 0.5
    }

    pub fn world_radius(&self) -> f32 {
        (self.world_max() - self.world_min()).length() * 0.5
    }
}

impl MaterialExtension for SceneBound {
    type Base = StandardMaterial;

    fn fragment_shader() -> bevy::render::render_resource::ShaderRef {
        ShaderRef::Path("embedded://shaders/bound_material.wgsl".into())
    }

    fn alpha_mode(base_mode: AlphaMode) -> Option<AlphaMode> {
        Some(match base_mode {
            AlphaMode::Opaque => AlphaMode::Mask(0.0),
            other => other,
        })
    }

    fn prepass_fragment_shader() -> ShaderRef {
        ShaderRef::Path("embedded://shaders/bound_prepass.wgsl".into())
    }

    fn fallback_asset(&self, base: &Self::Base) -> Option<ExtendedMaterial<Self>> {
        let base_color = Color::srgba(1.0, 1.0, 4.0, 1.0);
        let emissive = Color::srgba(1.0, 1.0, 4.0, 1.0);

        Some(ExtendedMaterial::<Self> {
            base: StandardMaterial {
                base_color,
                emissive: emissive.to_linear(),
                double_sided: true,
                cull_mode: base.cull_mode,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..Default::default()
            },
            extension: self.clone(),
        })
    }

    fn specialize(
        _: &bevy::pbr::MaterialExtensionPipeline,
        descriptor: &mut bevy::render::render_resource::RenderPipelineDescriptor,
        _: &bevy::render::mesh::MeshVertexBufferLayoutRef,
        key: bevy::pbr::MaterialExtensionKey<Self>,
    ) -> Result<(), bevy::render::render_resource::SpecializedMeshPipelineError> {
        let data = key.bind_group_data;

        if descriptor.primitive.cull_mode.is_some() {
            descriptor.primitive.cull_mode = if data.inverted_scale {
                Some(Face::Front)
            } else {
                Some(Face::Back)
            };
        }

        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "SHOW_OUTSIDE_BOUNDS_MESH_TAG".to_owned(),
                SCENE_MATERIAL_SHOW_OUTSIDE_BOUNDS_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "NO_DITHERING_MESH_TAG".to_owned(),
                SCENE_MATERIAL_NO_DITHERING_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "CONE_ONLY_DITHER_MESH_TAG".to_owned(),
                SCENE_MATERIAL_CONE_ONLY_DITHER_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "TOON_MESH_TAG".to_owned(),
                SCENE_MATERIAL_TOON_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "OUTLINE_MESH_TAGS".to_owned(),
                SCENE_MATERIAL_OUTLINE_MESH_TAGS,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "OUTLINE_BLACK_MESH_TAG".to_owned(),
                SCENE_MATERIAL_OUTLINE_BLACK_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "OUTLINE_RED_MESH_TAG".to_owned(),
                SCENE_MATERIAL_OUTLINE_RED_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "OUTLINE_GREEN_MESH_TAG".to_owned(),
                SCENE_MATERIAL_OUTLINE_GREEN_MESH_TAG,
            ));
            fragment.shader_defs.push(ShaderDefVal::UInt(
                "OUTLINE_BLUE_MESH_TAG".to_owned(),
                SCENE_MATERIAL_OUTLINE_BLUE_MESH_TAG,
            ));

            if data.inverted_scale {
                fragment.shader_defs.push("INVERTED_SCALE".into());
            }
        }
        Ok(())
    }

    // fn shadow_material_key(&self, base_key: Option<u64>) -> Option<u64> {
    //     base_key.map(|_| (((self.bounds.x as i64) << 32) | (self.bounds.y as i64)) as u64)
    // }
}

impl ImposterBakeMaterialExtension for SceneBound {
    fn imposter_fragment_shader() -> bevy::render::render_resource::ShaderRef {
        "embedded://shaders/bound_material_baker.wgsl".into()
    }
}
pub struct SceneBoundPlugin;

impl Plugin for SceneBoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialExtPlugin::<SceneMaterial>::default());

        app.init_resource::<InvertedMaterials>();
        // Default false; the app entry overrides it.
        app.init_resource::<ShowOutOfBounds>();

        app.add_observer(new_material);
        app.add_systems(
            PostUpdate,
            (new_materials, clear_old_materials)
                .chain()
                .after(TransformSystem::TransformPropagate),
        );
    }

    fn finish(&self, app: &mut App) {
        // not present when running without rendering
        if let Some(mut buffers) = app
            .world_mut()
            .get_resource_mut::<Assets<ShaderStorageBuffer>>()
        {
            // header + one (unused) cell word: a runtime-sized array binding can't be empty
            buffers.insert(
                EMPTY_SCENE_CELLS.id(),
                ShaderStorageBuffer::new(&[0; 8], RenderAssetUsages::RENDER_WORLD),
            );
        }
    }
}

pub struct MaterialExtPlugin<T: ImposterBakeMaterial> {
    _material_phantom: PhantomData<T>,
}

impl<T: ImposterBakeMaterial> Default for MaterialExtPlugin<T> {
    fn default() -> Self {
        Self {
            _material_phantom: PhantomData,
        }
    }
}

impl<T: ImposterBakeMaterial> Plugin for MaterialExtPlugin<T>
where
    MaterialPlugin<T>: Plugin,
    ImposterBakeMaterialPlugin<T>: Plugin,
{
    fn build(&self, app: &mut App) {
        app.register_required_components::<MeshMaterial3d<T>, MeshTag>();

        app.add_plugins(MaterialPlugin::<T>::default());
        let preview_mode = app
            .world()
            .get_resource::<PreviewMode>()
            .is_some_and(|p| p.is_preview);
        if !preview_mode {
            app.add_plugins(ImposterBakeMaterialPlugin::<T>::default());
        }

        app.add_observer(update_show_outside_bounds::<T>);
        app.add_observer(scene_material_removed::<T>);
    }
}

fn update_show_outside_bounds<T: Material>(
    trigger: Trigger<OnInsert, MeshMaterial3d<T>>,
    mut meshes: Query<&mut MeshTag, With<MeshMaterial3d<T>>>,
    show_oob: Res<ShowOutOfBounds>,
) {
    // Tag the mesh to render out-of-bounds instead of being culled.
    if !show_oob.0 {
        return;
    }

    let entity = trigger.target();
    let Ok(mut mesh_tag) = meshes.get_mut(entity) else {
        return;
    };
    mesh_tag.0 |= SCENE_MATERIAL_SHOW_OUTSIDE_BOUNDS_MESH_TAG;
}

#[derive(Debug, Default, Resource, Deref, DerefMut)]
pub struct InvertedMaterials {
    pub mapping: HashMap<Handle<SceneMaterial>, Handle<SceneMaterial>>,
}

#[derive(Debug, Default, Component)]
#[component(storage = "SparseSet")]
pub struct AddedSceneMaterial;

fn new_material(trigger: Trigger<OnInsert, MeshMaterial3d<SceneMaterial>>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(AddedSceneMaterial);
}

fn new_materials(
    mut commands: Commands,
    materials: Populated<
        (Entity, &GlobalTransform, &MeshMaterial3d<SceneMaterial>),
        With<AddedSceneMaterial>,
    >,
    mut inverted_materials: ResMut<InvertedMaterials>,
    mut scene_material_assets: ResMut<Assets<SceneMaterial>>,
) {
    for (entity, gt, mesh_material) in materials.into_inner() {
        commands.entity(entity).try_remove::<AddedSceneMaterial>();

        if gt.is_inverted() {
            match inverted_materials.entry(mesh_material.clone_weak()) {
                Entry::Vacant(vacant) => {
                    let Some(scene_material_asset) =
                        scene_material_assets.get(mesh_material.id()).filter(
                            |scene_material_asset| !scene_material_asset.extension.inverted_scale,
                        )
                    else {
                        debug!("{entity}'s material was invalid");
                        continue;
                    };

                    let mut inverted_material = scene_material_asset.clone();
                    inverted_material.extension.inverted_scale = true;
                    let new_handle = scene_material_assets.add(inverted_material);

                    vacant.insert(new_handle.clone());
                    commands
                        .entity(entity)
                        .try_insert(MeshMaterial3d(new_handle));
                }
                Entry::Occupied(occupied) => {
                    commands
                        .entity(entity)
                        .try_insert(MeshMaterial3d(occupied.get().clone()));
                }
            }
        }
    }
}

fn clear_old_materials(
    mut inverted_materials: ResMut<InvertedMaterials>,
    scene_material_assets: Res<Assets<SceneMaterial>>,
) {
    let len = inverted_materials.len();
    inverted_materials.retain(|k, _| scene_material_assets.contains(k.id()));
    if inverted_materials.len() != len {
        debug!(
            "Cleared {} inverted materials.",
            len - inverted_materials.len()
        );
    }
}

fn scene_material_removed<T: Material>(
    trigger: Trigger<OnRemove, MeshMaterial3d<T>>,
    mut commands: Commands,
) {
    commands.entity(trigger.target()).try_remove::<MeshTag>();
}

#[cfg(test)]
mod test {
    use bevy::math::{IVec2, Vec2, Vec3, Vec3Swizzles};

    use crate::BoundRegion;

    #[test]
    fn test_bounds() {
        for x in [-10, 0, 10] {
            for y in [-10, 0, 10] {
                let region = BoundRegion::new(IVec2::new(x, y), IVec2::new(x, y), 1);

                println!(
                    "[{},{}] -> {:x},{:x} -> {}, {}",
                    x,
                    y,
                    region.min,
                    region.max,
                    region.world_min(),
                    region.world_max()
                );
                assert_eq!(
                    region.world_min(),
                    Vec3::new(x as f32 * 16.0, 0.0, (-y - 1) as f32 * 16.0)
                );
                assert_eq!(
                    region.world_max(),
                    Vec3::new((x + 1) as f32 * 16.0, 20.0, -y as f32 * 16.0)
                );
                assert_eq!(region.parcel_min(), IVec2::new(x, y));
                assert_eq!(region.parcel_max(), IVec2::new(x, y));
            }
        }
    }

    // cpu ports of scene_bounds.wgsl (new) and the previous per-rect loop (old)
    fn new_amount(
        bounds: &super::SceneBounds,
        buffers: &bevy::asset::Assets<super::ShaderStorageBuffer>,
        distance: f32,
        pos: Vec3,
    ) -> f32 {
        let d = &bounds.data;
        let words: Vec<u32> = buffers
            .get(&bounds.cells)
            .and_then(|b| b.data.as_ref())
            .map(|bytes| {
                bytes
                    .chunks(4)
                    .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
                    .collect()
            })
            .unwrap_or_default();
        let region_at = |cell: IVec2| -> u32 {
            let local = cell - d.origin;
            if local.x < 0
                || local.y < 0
                || local.x as u32 >= d.size.x
                || local.y as u32 >= d.size.y
            {
                return 0;
            }
            let ix = local.y as u32 * d.size.x + local.x as u32;
            (words[(1 + words[0] + (ix >> 1)) as usize] >> ((ix & 1) * 16)) & 0xFFFF
        };
        let height = |r: u32| f32::from_bits(words[r as usize]);

        if d.flags & super::SCENE_BOUNDED == 0 {
            return 0.0;
        }
        let p = pos.xz();
        if d.flags & super::SCENE_FULL_RECT != 0 {
            let min_wp = d.origin.as_vec2() * 16.0;
            let max_wp = min_wp + d.size.as_vec2() * 16.0;
            let o = (p.clamp(min_wp, max_wp) - p).abs();
            return o.x.max(o.y).max(pos.y - d.rect_height);
        }
        let cell = (p / 16.0).floor().as_ivec2();
        let r = region_at(cell);
        if r != 0 {
            return (pos.y - height(r)).max(0.0);
        }
        let local = p - cell.as_vec2() * 16.0;
        let step = IVec2::new(
            if local.x >= 8.0 { 1 } else { -1 },
            if local.y >= 8.0 { 1 } else { -1 },
        );
        let edge = Vec2::new(
            if local.x >= 8.0 {
                16.0 - local.x
            } else {
                local.x
            },
            if local.y >= 8.0 {
                16.0 - local.y
            } else {
                local.y
            },
        );
        let reach = distance.max(0.05);
        let mut amount = 9999.0f32;
        let mut check = |off: IVec2, dist: f32| {
            let r = region_at(cell + off);
            if r != 0 {
                amount = amount.min(dist.max(pos.y - height(r)));
            }
        };
        if edge.x < reach {
            check(IVec2::new(step.x, 0), edge.x);
        }
        if edge.y < reach {
            check(IVec2::new(0, step.y), edge.y);
            if edge.x < reach {
                check(step, edge.x.max(edge.y));
            }
        }
        amount
    }

    fn old_amount(regions: &[BoundRegion], pos: Vec3) -> f32 {
        let unpack = |packed: u32| {
            let v = |x: i32| if x & 0x8000 != 0 { x - 0x10000 } else { x };
            Vec2::new(
                v(((packed >> 16) & 0xFFFF) as i32) as f32 * 16.0,
                v((packed & 0xFFFF) as i32) as f32 * 16.0,
            )
        };
        let (mut outside, mut nearest, mut nearest_height) = (9999.0f32, 9999.0f32, 9999.0f32);
        for r in regions {
            let o = (pos.xz().clamp(unpack(r.min), unpack(r.max)) - pos.xz()).abs();
            let distance = o.x.max(o.y);
            if distance < nearest {
                nearest = distance;
                nearest_height = r.height;
            }
            outside = outside.min(distance);
        }
        outside.max((pos.y - nearest_height).max(0.0))
    }

    #[test]
    fn parcel_index_matches_rect_loop() {
        use common::bounds_calc::scene_regions;

        // an L with a hole, a disconnected pair (different height), and a plain rectangle
        let shapes: Vec<Vec<IVec2>> = vec![
            (0..4)
                .flat_map(|x| (0..4).map(move |y| IVec2::new(x - 20, y + 30)))
                .filter(|p| *p != IVec2::new(-19, 31) && !(p.x >= -18 && p.y >= 32))
                .chain([IVec2::new(-10, 40), IVec2::new(-10, 41)])
                .collect(),
            (0..3)
                .flat_map(|x| (0..2).map(move |y| IVec2::new(x + 5, y - 7)))
                .collect(),
        ];

        let mut seed = 12345u32;
        let mut rand = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / (1 << 24) as f32
        };

        for parcels in shapes {
            let regions = scene_regions(parcels.iter().copied())
                .into_iter()
                .map(|r| BoundRegion::new(r.min, r.max, r.count))
                .collect::<Vec<_>>();
            let mut buffers = bevy::asset::Assets::<super::ShaderStorageBuffer>::default();
            let bounds = super::SceneBounds::new(&regions, Some(&mut buffers));

            let (min, max) = regions
                .iter()
                .map(BoundRegion::cells)
                .fold((IVec2::MAX, IVec2::MIN), |(a, b), (c, d)| {
                    (a.min(c), b.max(d))
                });
            let lo = (min - IVec2::ONE).as_vec2() * 16.0;
            let hi = (max + IVec2::splat(2)).as_vec2() * 16.0;
            for distance in [0.0, 2.0] {
                let reach = f32::max(distance, 0.05);
                for _ in 0..20000 {
                    let xz = lo + (hi - lo) * Vec2::new(rand(), rand());
                    let pos = Vec3::new(xz.x, rand() * 80.0 - 10.0, xz.y);
                    let old = old_amount(&regions, pos);
                    let new = new_amount(&bounds, &buffers, distance, pos);
                    if old < reach {
                        assert!((old - new).abs() < 1e-4, "{pos} old {old} new {new}");
                    } else {
                        assert!(new >= reach, "{pos} old {old} new {new}");
                    }
                }
            }
        }
    }
}
