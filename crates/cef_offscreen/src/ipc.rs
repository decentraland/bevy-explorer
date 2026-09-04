mod host_emit;
mod js_emit;

use crate::ipc::js_emit::IpcRawEventPlugin;
use bevy::prelude::*;

use crate::ipc::host_emit::HostEmitPlugin;
pub use host_emit::*;
pub use js_emit::*;

pub struct IpcPlugin;

impl Plugin for IpcPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((IpcRawEventPlugin, HostEmitPlugin));
    }
}
