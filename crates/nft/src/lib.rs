pub mod asset_source;

use std::{f32::consts::FRAC_PI_2, path::PathBuf};

use asset_source::{Nft, NftLoader};
use bevy::{
    asset::LoadState,
    gltf::Gltf,
    prelude::*,
    utils::{HashMap, HashSet},
};
use common::{sets::SceneSets, util::TryPushChildrenEx};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::sdk::components::{NftFrameType, PbNftShape},
    SceneComponentId,
};
use ipfs::ipfs_path::IpfsPath;
use once_cell::sync::Lazy;
use scene_runner::update_world::AddCrdtInterfaceExt;

pub struct NftShapePlugin;

impl Plugin for NftShapePlugin {
    fn build(&self, app: &mut App) {
        app.register_asset_loader(NftLoader);
        app.init_asset::<Nft>();
        app.add_crdt_lww_component::<PbNftShape, NftShape>(
            SceneComponentId::NFT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (update_nft_shapes, load_frame, load_nft, resize_nft).in_set(SceneSets::PostLoop),
        );
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
    query: Query<(Entity, &NftShape), Changed<NftShape>>,
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
    for (ent, nft_shape) in query.iter() {
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
                c.spawn((SpatialBundle::default(), FrameLoading(nft_shape.0.style())));

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

        scene_spawner.spawn_as_child(gltf.default_scene.as_ref().unwrap(), child);
        commands
            .entity(ent)
            .remove::<FrameLoading>()
            .try_push_children(&[child]);
    }
}

#[derive(Component)]
pub struct NftLoading(Handle<Nft>);

fn load_nft(
    mut commands: Commands,
    q: Query<(Entity, &NftLoading)>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut mesh: Local<Option<Handle<Mesh>>>,
    nfts: Res<Assets<Nft>>,
) {
    for (ent, nft) in q.iter() {
        let Some(nft) = nfts.get(nft.0.id()) else {
            if asset_server.load_state(nft.0.id()) == LoadState::Failed {
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

        commands
            .entity(ent)
            .try_insert((
                PbrBundle {
                    transform: Transform::from_translation(Vec3::Z * 0.03),
                    mesh: mesh
                        .get_or_insert_with(|| meshes.add(shape::Quad::default().into()))
                        .clone(),
                    material: materials.add(StandardMaterial {
                        base_color_texture: Some(h_image.clone()),
                        ..Default::default()
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
