//! Wasm Pulse driver — a no-op until WebTransport lands.
//!
//! The wasm client has no ENet. For now this does nothing and foreign players continue to arrive
//! over LiveKit. When WebTransport is available this becomes an async task that pumps the same
//! [`PulseDriverChannels`] behind the identical byte boundary — only this file changes.

use super::transport::{PulseDriverChannels, PulseTransportConfig};

pub(super) fn spawn(config: PulseTransportConfig, _channels: PulseDriverChannels) {
    // Dropping `_channels` closes the driver ends; the protocol layer simply sees no inbound
    // traffic and no status — a clean inert session.
    bevy::log::info!(
        "pulse: wasm driver is a no-op until WebTransport is available (target {}:{})",
        config.host,
        config.port
    );
}
