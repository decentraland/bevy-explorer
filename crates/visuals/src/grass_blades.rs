use bevy::{
    asset::{embedded_asset, embedded_path, weak_handle, RenderAssetUsages},
    math::Vec3A,
    pbr::NotShadowCaster,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::mesh::MeshAabb,
    render::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology},
    render::render_resource::ShaderRef,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    tasks::Task,
};
use common::{structs::ParcelGrassConfig, terrain::TerrainField, util::TaskExt};

use crate::grass_look::{GrassLook, GrassLookPlugin};
use crate::{ground_mask::GroundMask, shell_texturing::ParcelGrassShell};

pub(super) const MATERIAL: Handle<GrassMaterial> =
    weak_handle!("ea376d98-b05a-48c3-8a60-d6706ec8bed9");

pub(super) struct GrassBladesPlugin;

impl Plugin for GrassBladesPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "grass_blades.wgsl");
        app.add_plugins((MaterialPlugin::<GrassMaterial>::default(), GrassLookPlugin))
            .add_systems(Startup, setup_cutouts)
            .add_systems(Update, finish_patches)
            .add_systems(
                PostUpdate,
                (
                    update_cutouts.run_if(resource_changed::<GrassLook>),
                    update_material.after(crate::ground_mask::update).run_if(
                        resource_changed::<ParcelGrassConfig>
                            .or(resource_changed::<GroundMask>)
                            .or(resource_changed::<GrassLook>),
                    ),
                )
                    .chain(),
            );
    }
}

#[derive(Clone, Asset, TypePath, AsBindGroup)]
pub(super) struct GrassMaterial {
    #[uniform(0)]
    root_color: LinearRgba,
    #[uniform(1)]
    tip_color: LinearRgba,
    #[texture(2, sample_type = "u_int")]
    ground_mask: Handle<Image>,
    #[texture(3, dimension = "2d_array")]
    #[sampler(4)]
    leaf_mask: Handle<Image>,
    #[uniform(5)]
    form: Vec4,
    #[uniform(6)]
    shading: Vec4,
    #[uniform(7)]
    ground_detail: Vec4,
    #[texture(8)]
    #[sampler(9)]
    meadow: Handle<Image>,
    #[storage(101, read_only)]
    ambient: Handle<bevy::render::storage::ShaderStorageBuffer>,
}

#[derive(Resource)]
struct GrassCutouts(Handle<Image>);

fn setup_cutouts(mut commands: Commands, mut images: ResMut<Assets<Image>>, look: Res<GrassLook>) {
    commands.insert_resource(GrassCutouts(
        images.add(crate::grass_cutouts::image(look.leaves)),
    ));
}

fn update_cutouts(
    cutouts: Res<GrassCutouts>,
    mut images: ResMut<Assets<Image>>,
    look: Res<GrassLook>,
) {
    images.insert(cutouts.0.id(), crate::grass_cutouts::image(look.leaves));
}

impl Material for GrassMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path(
            format!(
                "embedded://{}",
                embedded_path!("grass_blades.wgsl").display()
            )
            .into(),
        )
    }

    fn fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        Self::vertex_shader()
    }

    fn prepass_fragment_shader() -> ShaderRef {
        Self::vertex_shader()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Mask(0.5)
    }

    // Bevy's prepass fragment consumes the deformed positions/normals, including motion vectors.
    fn specialize(
        _: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        _: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

fn update_material(
    mut materials: ResMut<Assets<GrassMaterial>>,
    config: Res<ParcelGrassConfig>,
    mask: Res<GroundMask>,
    cutouts: Res<GrassCutouts>,
    look: Res<GrassLook>,
) {
    materials.insert(
        MATERIAL.id(),
        GrassMaterial {
            root_color: config.root_color.to_linear(),
            tip_color: config.tip_color.to_linear(),
            ground_mask: mask.0.clone(),
            leaf_mask: cutouts.0.clone(),
            form: look.form,
            shading: look.shading,
            ground_detail: look.ground(),
            ambient: crate::unity_ambient::UNITY_AMBIENT_BUFFER,
            meadow: crate::meadow_texture::MEADOW,
        },
    );
}

#[derive(Component)]
pub(super) struct PendingGrass(pub Task<Mesh>);

fn finish_patches(
    mut commands: Commands,
    mut patches: Query<(Entity, &mut PendingGrass)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Bound main-thread uploads when a teleport makes many jobs ready together.
    let mut uploaded = 0;
    for (entity, mut pending) in &mut patches {
        let Some(mesh) = pending.0.complete() else {
            continue;
        };
        if let Some(mut bounds) = mesh.compute_aabb() {
            bounds.half_extents += Vec3A::splat(0.35);
            commands.entity(entity).insert((
                Name::new("Red grass blades"),
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(MATERIAL),
                ParcelGrassShell,
                NotShadowCaster,
                bounds,
            ));
        }
        commands.entity(entity).remove::<PendingGrass>();
        uploaded += 1;
        if uploaded == 4 {
            break;
        }
    }
}

pub(super) fn random(seed: u32) -> f32 {
    let mut x = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28) + 4)) ^ x).wrapping_mul(277_803_737);
    ((x >> 22) ^ x) as f32 / u32::MAX as f32
}

/// Two crossed cards per tuft; the shader cuts six curved leaves into each card.
/// Eight vertices replace 21 ribbon vertices, with stable roots across detail levels.
pub(super) fn build_patch(
    parcel: IVec2,
    config: ParcelGrassConfig,
    lod: usize,
    look: GrassLook,
    terrain: Option<&TerrainField>,
) -> Mesh {
    let grid = config.subdivisions.clamp(4, 40) * 3 / 2;
    let density = config.layers.min(32) as f32 / 32.0 / lod.max(1) as f32 * look.shading.w;
    let height = (config.layers as f32 * config.y_displacement * 0.8).clamp(0.08, 0.45);
    let center = Vec2::new(parcel.x as f32 * 16.0 + 8.0, -parcel.y as f32 * 16.0 - 8.0);
    let seed =
        (parcel.x as u32).wrapping_mul(73_856_093) ^ (parcel.y as u32).wrapping_mul(19_349_663);
    let mut positions = Vec::<[f32; 3]>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut uv = Vec::<[f32; 2]>::new();
    let mut roots = Vec::<[f32; 2]>::new();
    let mut base_heights = Vec::<[f32; 4]>::new();
    let mut indices = Vec::<u32>::new();
    for cell in 0..grid * grid {
        let seed = seed ^ cell.wrapping_mul(83_492_791);
        if random(seed ^ 0x82a4) >= density {
            continue;
        }
        let x = ((cell % grid) as f32 + 0.2 + random(seed) * 0.6) * 16.0 / grid as f32 - 8.0;
        let z =
            ((cell / grid) as f32 + 0.2 + random(seed ^ 0xa129) * 0.6) * 16.0 / grid as f32 - 8.0;
        let root = Vec2::new(x, z).clamp(Vec2::splat(-7.65), Vec2::splat(7.65));
        let r = random(seed ^ 0x4f71);
        for card in 0..2u32 {
            let angle = r * std::f32::consts::TAU + card as f32 * std::f32::consts::FRAC_PI_2;
            let across = Vec3::new(angle.cos(), 0.0, angle.sin());
            let h = height * (0.8 + 0.5 * r);
            let width = 0.20 + 0.07 * random(seed ^ 73);
            let ends = [-1.0, 1.0].map(|side| root + across.xz() * width * side);
            if ends.iter().any(|end| {
                terrain.is_some_and(|field| {
                    let world = *end + center;
                    !field.contains(world.x, -world.y)
                })
            }) {
                continue;
            }
            let grounds = ends.map(|end| {
                let world = end + center;
                terrain.map_or(0.0, |field| field.height(world.x, -world.y)) - 0.015
            });
            let base = positions.len() as u32;
            for level in 0..2 {
                for side in 0..2 {
                    let end = ends[side];
                    positions.push([end.x, grounds[side] + h * level as f32, end.y]);
                    // Ground-facing normals avoid dark, alternating card faces.
                    normals.push(Vec3::Y.to_array());
                    uv.push([side as f32, level as f32]);
                    roots.push(end.to_array());
                    base_heights.push([grounds[side], r, card as f32, 1.0]);
                }
            }
            indices.extend([0, 1, 2, 1, 3, 2].map(|i| base + i));
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, roots)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, base_heights)
        .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    fn positions(mesh: &Mesh) -> &Vec<[f32; 3]> {
        let Some(VertexAttributeValues::Float32x3(values)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        values
    }

    #[test]
    fn patches_are_deterministic_bounded_and_lods_keep_the_same_blades() {
        let config = ParcelGrassConfig::default();
        let parcel = IVec2::new(-7, 3);
        let high = build_patch(parcel, config, 1, GrassLook::default(), None);
        let low = build_patch(parcel, config, 4, GrassLook::default(), None);
        assert_eq!(
            positions(&high),
            positions(&build_patch(parcel, config, 1, GrassLook::default(), None))
        );
        assert!(high.count_vertices() <= 32 * 32 * 3 * 7);
        assert!(low.count_vertices() > 0 && low.count_vertices() < high.count_vertices());
        let key = |card: &[[f32; 3]]| {
            card.iter()
                .flat_map(|point| point.map(f32::to_bits))
                .collect::<Vec<_>>()
        };
        let high_cards: std::collections::HashSet<_> =
            positions(&high).chunks_exact(4).map(key).collect();
        for card in positions(&low).chunks_exact(4) {
            assert!(high_cards.contains(&key(card)));
        }
        assert!(positions(&high)
            .iter()
            .all(|p| p.iter().all(|v| v.is_finite()) && p[0].abs() < 8.0 && p[2].abs() < 8.0));
        assert!(high
            .indices()
            .unwrap()
            .iter()
            .all(|i| i < high.count_vertices()));
        assert_eq!(
            build_patch(
                parcel,
                ParcelGrassConfig {
                    layers: 0,
                    ..config
                },
                1,
                GrassLook::default(),
                None
            )
            .count_vertices(),
            0
        );
    }

    #[test]
    fn roots_follow_terrain_including_negative_parcel_coordinates() {
        let parcels: Vec<_> = (0..20).flat_map(|x| (0..20).map(move |y| [x, y])).collect();
        let terrain = TerrainField::from_parcels(&parcels, true).unwrap();
        let parcel = IVec2::new(-3, 2);
        let mesh = build_patch(
            parcel,
            ParcelGrassConfig::default(),
            1,
            GrassLook::default(),
            Some(&terrain),
        );
        assert!(mesh.count_vertices() > 0);
        assert!(positions(&mesh).iter().any(|p| p[1] > 0.5));
        for card in positions(&mesh).chunks_exact(4) {
            for point in &card[..2] {
                let root = Vec3::from_array(*point);
                let world = Transform::from_xyz(
                    parcel.x as f32 * 16.0 + 8.0,
                    0.0,
                    -parcel.y as f32 * 16.0 - 8.0,
                )
                .transform_point(root);
                let expected = terrain.height(world.x, -world.z);
                assert!(
                    (root.y - expected + 0.015).abs() < 0.0001,
                    "root={root:?} world={world:?} height={expected}"
                );
            }
        }
        assert_eq!(
            build_patch(
                IVec2::splat(500),
                ParcelGrassConfig::default(),
                1,
                GrassLook::default(),
                Some(&terrain)
            )
            .count_vertices(),
            0
        );
    }

    #[test]
    fn tuft_cards_reduce_geometry_and_keep_low_and_high_the_same_height() {
        let config = ParcelGrassConfig::default();
        let high = build_patch(IVec2::ZERO, config, 1, GrassLook::default(), None);
        let low = build_patch(
            IVec2::ZERO,
            ParcelGrassConfig {
                layers: 8,
                y_displacement: 0.04,
                ..config
            },
            1,
            GrassLook::default(),
            None,
        );
        // Compared with the old three seven-vertex ribbons per grid cell.
        assert!(high.count_vertices() < 32 * 32 * 3 * 7);
        assert!(high.indices().unwrap().len() < 32 * 32 * 3 * 15);
        let high_tips: std::collections::HashSet<_> = positions(&high)
            .iter()
            .map(|p| p.map(f32::to_bits))
            .collect();
        assert!(positions(&low)
            .iter()
            .all(|p| high_tips.contains(&p.map(f32::to_bits))));
        assert!(positions(&high).iter().all(|p| p[1] <= 0.34));
        assert!(
            low.count_vertices() > 200,
            "Low must retain sparse near-field detail"
        );
    }
}
