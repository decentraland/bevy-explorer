use std::sync::mpsc::SyncSender;

use dcl::{interface::CrdtComponentInterfaces, RendererResponse, SceneId, SceneResponse};
use ipfs::{IpfsResource, SceneJsFile};
use system_bridge::SystemApi;
use tokio::sync::mpsc::Sender;
use wallet::Wallet;

pub fn init_runtime() {}

#[allow(clippy::too_many_arguments)]
pub fn spawn_scene(
    _scene_hash: String,
    _scene_js: SceneJsFile,
    _crdt_component_interfaces: CrdtComponentInterfaces,
    _renderer_sender: SyncSender<SceneResponse>,
    _global_update_receiver: tokio::sync::broadcast::Receiver<Vec<u8>>,
    _ipfs: IpfsResource,
    _wallet: Wallet,
    _id: SceneId,
    _storage_root: String,
    _inspect: bool,
    _testing: bool,
    _preview: bool,
    _super_user: Option<tokio::sync::mpsc::UnboundedSender<SystemApi>>,
) -> Sender<RendererResponse> {
    todo!()
}
