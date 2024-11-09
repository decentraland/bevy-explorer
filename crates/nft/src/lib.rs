pub mod asset_source;
mod extended_image_loader;

use std::{f32::consts::FRAC_PI_2, path::PathBuf};

use asset_source::{Nft, NftLoader};
use bevy::{
    asset::LoadState,
    gltf::Gltf,
    prelude::*,
    scene::InstanceId,
    utils::{HashMap, HashSet},
};
use common::{sets::SceneSets, structs::AppConfig, util::TryPushChildrenEx};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{NftFrameType, PbNftShape},
    SceneComponentId,
};
use extended_image_loader::ExtendedImageLoader;
use ipfs::ipfs_path::IpfsPath;
use once_cell::sync::Lazy;
use scene_material::{SceneBound, SceneMaterial};
use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, SceneEntity,
};

pub struct NftShapePlugin;

impl Plugin for NftShapePlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(NftLoader);
        app.preregister_asset_loader::<ExtendedImageLoader>(&["image"]);
        app.init_asset::<Nft>();
        app.add_crdt_lww_component::<PbNftShape, NftShape>(
            SceneComponentId::NFT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (
                update_nft_shapes,
                load_frame,
                process_frame,
                load_nft,
                resize_nft,
            )
                .in_set(SceneSets::PostLoop),
        );
    }

    fn finish(&self, app: &mut App) {
        app.init_asset_loader::<ExtendedImageLoader>();
    }
}

#[derive(Component)]
pub struct NftShape(pub PbNftShape);

impl From<PbNftShape> for NftShape {
    fn from(value: PbNftShape) -> Self {
        Self(value)
    }
}

#[derive(Component)]
pub struct NftShapeMarker;

#[derive(Component)]
pub struct RetryNftShape;

fn update_nft_shapes(
    mut commands: Commands,
    query: Query<(Entity, &NftShape, &SceneEntity), Changed<NftShape>>,
    existing: Query<(Entity, &Parent), With<NftShapeMarker>>,
    mut removed: RemovedComponents<NftShape>,
    asset_server: Res<AssetServer>,
) {
    // remove changed and deleted nodes
    let old_parents = query
        .iter()
        .map(|(e, ..)| e)
        .chain(removed.read())
        .collect::<HashSet<_>>();
    for (ent, par) in existing.iter() {
        if old_parents.contains(&par.get()) {
            commands.entity(ent).despawn_recursive();
        }
    }

    // add new nodes
    for (ent, nft_shape, scene_ent) in query.iter() {
        // spawn parent
        let nft_ent = commands
            .spawn((
                SpatialBundle {
                    transform: Transform::from_scale(Vec3::new(0.5, 0.5, 1.0)),
                    ..Default::default()
                },
                NftShapeMarker,
            ))
            .with_children(|c| {
                // spawn frame
                c.spawn((
                    SpatialBundle::default(),
                    FrameLoading(nft_shape.0.style()),
                    scene_ent.clone(),
                ));

                // spawn content
                c.spawn((
                    SpatialBundle::default(),
                    NftLoading(asset_server.load(format!(
                        "nft://{}.nft",
                        urlencoding::encode(&nft_shape.0.urn)
                    ))),
                ));
            })
            .id();

        commands.entity(ent).try_push_children(&[nft_ent]);
    }
}

#[derive(Component)]
pub struct FrameLoading(NftFrameType);

#[derive(Component)]
pub struct FrameProcess(InstanceId);

fn load_frame(
    mut commands: Commands,
    q: Query<(Entity, &FrameLoading)>,
    asset_server: Res<AssetServer>,
    mut gltf_handles: Local<HashMap<NftFrameType, Handle<Gltf>>>,
    gltfs: Res<Assets<Gltf>>,
    mut scene_spawner: ResMut<SceneSpawner>,
) {
    for (ent, frame) in q.iter() {
        // get frame
        let h_gltf = gltf_handles
            .entry(frame.0)
            .or_insert_with(|| asset_server.load(*NFTSHAPE_LOOKUP.get(&frame.0).unwrap()));
        let Some(gltf) = gltfs.get(h_gltf.id()) else {
            debug!("waiting for frame");
            continue;
        };

        // \o/
        let transform = if frame.0 == NftFrameType::NftClassic {
            Transform::IDENTITY
        } else {
            Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))
        };

        let child = commands
            .spawn(SpatialBundle {
                transform,
                ..Default::default()
            })
            .id();

        let instance = scene_spawner.spawn_as_child(gltf.default_scene.clone().unwrap(), child);
        commands
            .entity(ent)
            .remove::<FrameLoading>()
            .try_insert(FrameProcess(instance))
            .try_push_children(&[child]);
    }
}

#[allow(clippy::too_many_arguments)]
fn process_frame(
    mut commands: Commands,
    q: Query<(Entity, &FrameProcess, &SceneEntity)>,
    scene_spawner: Res<SceneSpawner>,
    mat_nodes: Query<&Handle<StandardMaterial>>,
    mats: ResMut<Assets<StandardMaterial>>,
    mut new_mats: ResMut<Assets<SceneMaterial>>,
    scenes: Query<&RendererSceneContext>,
    config: Res<AppConfig>,
) {
    for (ent, frame, scene_ent) in q.iter() {
        if scene_spawner.instance_is_ready(frame.0) {
            commands.entity(ent).remove::<FrameProcess>();
            let Ok(bounds) = scenes.get(scene_ent.root).map(|ctx| ctx.bounds.clone()) else {
                continue;
            };
            for spawned_ent in scene_spawner.iter_instance_entities(frame.0) {
                if let Some(mat) = mat_nodes
                    .get(spawned_ent)
                    .ok()
                    .and_then(|h_mat| mats.get(h_mat))
                {
                    commands
                        .entity(spawned_ent)
                        .remove::<Handle<StandardMaterial>>()
                        .try_insert(new_mats.add(SceneMaterial {
                            base: mat.clone(),
                            extension: SceneBound::new(bounds.clone(), config.graphics.oob),
                        }));
                }
            }
        }
    }
}

#[derive(Component)]
pub struct NftLoading(Handle<Nft>);

#[allow(clippy::too_many_arguments)]
fn load_nft(
    mut commands: Commands,
    q: Query<(Entity, &SceneEntity, &NftLoading)>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<SceneMaterial>>,
    mut mesh: Local<Option<Handle<Mesh>>>,
    scenes: Query<&RendererSceneContext>,
    nfts: Res<Assets<Nft>>,
    config: Res<AppConfig>,
) {
    for (ent, scene_ent, nft) in q.iter() {
        let Some(nft) = nfts.get(nft.0.id()) else {
            if let LoadState::Failed(_) = asset_server.load_state(nft.0.id()) {
                debug!("nft failed");
                commands.entity(ent).remove::<NftLoading>();
            } else {
                debug!("waiting for nft");
            }
            continue;
        };

        // get image
        let url = &nft.image_url;
        let ipfs_path = IpfsPath::new_from_url(url, "image");
        let h_image = asset_server.load(PathBuf::from(&ipfs_path));

        // get bounds
        let Ok(bounds) = scenes.get(scene_ent.root).map(|ctx| ctx.bounds.clone()) else {
            continue;
        };

        commands
            .entity(ent)
            .try_insert((
                MaterialMeshBundle {
                    transform: Transform::from_translation(Vec3::Z * 0.03),
                    mesh: mesh
                        .get_or_insert_with(|| meshes.add(Rectangle::default()))
                        .clone(),
                    material: materials.add(SceneMaterial {
                        base: StandardMaterial {
                            base_color_texture: Some(h_image.clone()),
                            ..Default::default()
                        },
                        extension: SceneBound::new(bounds, config.graphics.oob),
                    }),
                    ..Default::default()
                },
                NftResize(h_image),
            ))
            .remove::<NftLoading>();
    }
}

#[derive(Component)]
pub struct NftResize(Handle<Image>);

fn resize_nft(
    mut commands: Commands,
    q: Query<(Entity, &Parent, &NftResize)>,
    images: Res<Assets<Image>>,
    mut transforms: Query<&mut Transform, With<NftShapeMarker>>,
) {
    for (ent, parent, resize) in q.iter() {
        if let Some(image) = images.get(resize.0.id()) {
            let max_dim = image.width().max(image.height()) as f32 * 2.0;
            let w = image.width() as f32 / max_dim;
            let h = image.height() as f32 / max_dim;

            if let Ok(mut transform) = transforms.get_mut(parent.get()) {
                transform.scale = Vec3::new(w, h, 1.0);
            }

            commands.entity(ent).remove::<NftResize>();
        }
    }
}

static NFTSHAPE_LOOKUP: Lazy<HashMap<NftFrameType, &'static str>> = Lazy::new(|| {
    use NftFrameType::*;
    HashMap::from_iter([
        (NftClassic, "nft_shapes/Classic.glb"),
        (NftBaroqueOrnament, "nft_shapes/Baroque_Ornament.glb"),
        (NftDiamondOrnament, "nft_shapes/Diamond_Ornament.glb"),
        (NftMinimalWide, "nft_shapes/Minimal_Wide.glb"),
        (NftMinimalGrey, "nft_shapes/Minimal_Grey.glb"),
        (NftBlocky, "nft_shapes/Blocky.glb"),
        (NftGoldEdges, "nft_shapes/Gold_Edges.glb"),
        (NftGoldCarved, "nft_shapes/Gold_Carved.glb"),
        (NftGoldWide, "nft_shapes/Gold_Wide.glb"),
        (NftGoldRounded, "nft_shapes/Gold_Rounded.glb"),
        (NftMetalMedium, "nft_shapes/Metal_Medium.glb"),
        (NftMetalWide, "nft_shapes/Metal_Wide.glb"),
        (NftMetalSlim, "nft_shapes/Metal_Slim.glb"),
        (NftMetalRounded, "nft_shapes/Metal_Rounded.glb"),
        (NftPins, "nft_shapes/Pins.glb"),
        (NftMinimalBlack, "nft_shapes/Minimal_Black.glb"),
        (NftMinimalWhite, "nft_shapes/Minimal_White.glb"),
        (NftTape, "nft_shapes/Tape.glb"),
        (NftWoodSlim, "nft_shapes/Wood_Slim.glb"),
        (NftWoodWide, "nft_shapes/Wood_Wide.glb"),
        (NftWoodTwigs, "nft_shapes/Wood_Twigs.glb"),
        (NftCanvas, "nft_shapes/Canvas.glb"),
        (NftNone, "nft_shapes/Classic.glb"),
    ])
});
