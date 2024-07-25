use std::{env::args, path::PathBuf, str::FromStr, sync::Arc};

use bevy::asset::io::AssetReader;
use bevy::{
    asset::{io::file::FileAssetReader, AsyncReadExt},
    core::TaskPoolOptions,
    ecs::entity::Entity,
    log::info,
    math::IVec2,
    transform::components::Transform,
    utils::HashMap,
};
use common::{
    structs::{IVec2Arg, SceneMeta},
    util::project_directories,
};
use dcl::{
    interface::{CrdtComponentInterfaces, CrdtStore, CrdtType},
    spawn_scene, SceneId,
};
use dcl_component::{
    transform_and_parent::DclTransformAndParent, DclReader, DclWriter, SceneComponentId,
    SceneEntityId,
};
use futures_lite::future::block_on;
use ipfs::{
    ipfs_path::{IpfsPath, IpfsType},
    EntityDefinition, EntityDefinitionLoader, IpfsIo, IpfsResource, SceneJsFile,
};

fn break_everything(parcel: IVec2, urn: Option<String>) {
    TaskPoolOptions::default().create_default_pools();

    // let parcel = IVec2::new(69, -3);
    info!("lets go, parcel = {parcel:?}");

    let default_reader = FileAssetReader::new("assets");
    let cache_root = project_directories().data_local_dir().join("cache");
    let ipfs_io = IpfsIo::new(
        true,
        Box::new(default_reader),
        cache_root,
        HashMap::default(),
        32,
    );
    let ipfs_io = Arc::new(ipfs_io);
    block_on(
        ipfs_io.set_realm(
            "https://sdk-team-cdn.decentraland.org/ipfs/goerli-plaza-main-latest"
                .to_owned(),
        ),
    );
    // block_on(ipfs_io.set_realm("https://realm-provider.decentraland.org/main".to_owned()));

    let entity = if let Some(urn) = urn {
        let ipfs_path = &IpfsPath::new_from_urn::<EntityDefinition>(&urn).unwrap();
        let pathbuf = PathBuf::from(ipfs_path);
        let mut reader = block_on(ipfs_io.read(&pathbuf)).unwrap();
        block_on(EntityDefinitionLoader.load_internal(&mut reader, &(), || {
            ipfs_path.context_free_hash().unwrap().unwrap()
        }))
        .unwrap()
    } else {
        let entities = block_on(ipfs_io.active_entities(
            ipfs::ActiveEntitiesRequest::Pointers(vec![format!("{},{}", parcel.x, parcel.y)]),
            None,
        ))
        .unwrap();
        entities.into_iter().next().unwrap()
    };

    let scene_hash = entity.id;
    info!("scene hash = {scene_hash}");

    let meta_str = entity.metadata.as_ref().unwrap().to_string();

    let meta = serde_json::from_value::<SceneMeta>(entity.metadata.unwrap()).unwrap();
    let scene_js_file = meta.main.clone();
    let is_sdk7 = meta.runtime_version == Some("7".to_owned());
    ipfs_io.add_collection(scene_hash.clone(), entity.content, None, Some(meta_str));

    let ipfs_path = PathBuf::from(&if is_sdk7 {
        IpfsPath::new(IpfsType::ContentFile {
            content_hash: scene_hash.clone(),
            file_path: scene_js_file.clone(),
        })
    } else {
        info!(
            "no sdk6 - expected {:?} found {:?}",
            Some("7".to_owned()),
            meta.runtime_version
        );
        info!("meta: {:#?}", meta);
        return;
        // IpfsPath::new_from_url("https://renderer-artifacts.decentraland.org/sdk6-adaption-layer/main/index.min.js", "js")
    });

    info!("opening js ({})", scene_js_file);
    let mut raw_scene_js = block_on(ipfs_io.read(&ipfs_path)).unwrap();
    info!("reading js");
    let mut bytes = Vec::default();
    block_on(raw_scene_js.read_to_end(&mut bytes)).unwrap();
    let scene_js = SceneJsFile(Arc::new(String::from_utf8(bytes).unwrap()));
    info!("loaded");

    let interfaces = CrdtComponentInterfaces::default();

    let (sx, rx) = std::sync::mpsc::sync_channel(1);

    let (_gusx, gurx) = tokio::sync::broadcast::channel(10);

    let ipfs_res = IpfsResource {
        inner: ipfs_io.clone(),
    };

    let wallet = wallet::Wallet::default();

    info!("spawning");

    let sender = spawn_scene(
        scene_hash,
        scene_js,
        interfaces,
        sx,
        gurx,
        ipfs_res,
        wallet,
        SceneId(Entity::from_raw(0)),
        false,
        false,
        false,
    );

    let mut crdt_store = CrdtStore::default();

    let mut buf = Vec::default();
    DclWriter::new(&mut buf).write(&DclTransformAndParent::from_bevy_transform_and_parent(
        &Transform::default(),
        SceneEntityId::ROOT,
    ));
    for id in [SceneEntityId::PLAYER, SceneEntityId::CAMERA] {
        crdt_store.update_if_different(
            SceneComponentId::TRANSFORM,
            CrdtType::LWW_ENT,
            id,
            Some(&mut DclReader::new(&buf)),
        );
    }

    loop {
        sender
            .blocking_send(dcl::RendererResponse::Ok(crdt_store.take_updates()))
            .unwrap();
        info!("sent");
        let received = rx.recv().unwrap();
        info!("received {:?}", received);
    }
}

fn main() {
    let parcel = IVec2Arg::from_str(args().nth(1).unwrap().as_ref()).unwrap();
    // let hash = args().nth(2);
    let hash = Some("urn:decentraland:entity:bafkreiaulpqp454jmiuy3puspafeenlfza2zspyorfmxs2f6sutcdcuqti?=&baseUrl=https://sdk-team-cdn.decentraland.org/ipfs/".to_owned());
    // bafybeig2np5mxe5zida6pwpf7mqrcgdaycvexxbuf5szwbqsqezln4a36e

    let default_filter = { format!("{},{}", bevy::log::Level::INFO, "wgpu=error,naga=error") };
    let filter_layer = bevy::log::tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| bevy::log::tracing_subscriber::EnvFilter::try_new(&default_filter))
        .unwrap();

    let l = bevy::log::tracing_subscriber::fmt()
        // .with_ansi(false)
        .with_env_filter(filter_layer)
        .finish();

    bevy::utils::tracing::subscriber::set_global_default(l).unwrap();
    break_everything(parcel.0, hash);
}
