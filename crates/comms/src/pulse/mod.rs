//! Pulse transport — read side.
//!
//! Pulse is the server-authoritative realtime movement backend (UDP/ENet). This module owns the
//! *decode + per-subject sliding-window state* and reconstructs each subject's full state into an
//! [`rfc4::Movement`], so that everything downstream (`global_crdt::update_player`,
//! `foreign_dynamics` interpolation, scene-driven-animation resolution) is reused verbatim — the
//! rest of the client never learns Pulse exists below the `PlayerUpdate` line.
//!
//! Two contracts are agreed out-of-band, not on the wire:
//!   * the quantization ABI (`{min,max,bits}` per delta field) — handled by the `*_dequantized()`
//!     accessors generated from the proto descriptor in `dcl_component`;
//!   * the parcel grid ([`PulseParcelGrid`]) — the server's `ParcelEncoder` config, which maps a
//!     `parcel_index` + in-parcel local position back to world coordinates.
//!
//! The animation rider that today rides on LiveKit `Movement` packets has no Pulse equivalent, so
//! the synthesized `Movement` carries `scene_driven_animation = None`; the real rider keeps
//! arriving over LiveKit and converges on the same wallet `Address`.

use std::collections::HashMap;

use bevy::math::Vec3;
use dcl_component::proto_components::{kernel::comms::rfc4, pulse};
use ethers_core::types::Address;

pub mod plugin;
pub mod transport;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
mod wasm;

/// World ↔ parcel mapping, mirroring the server's `ParcelEncoder`. The server folds `Padding` into
/// the bounds, so we recompute `min`/`width` exactly the same way rather than pre-baking a width —
/// a padding change on the server can't silently desync the decode.
///
/// These values are a per-server-instance deployment constant (one global grid shared across all
/// realms; `realm` only partitions visibility), not realm-derived and not transmitted.
#[derive(Debug, Clone, Copy)]
pub struct PulseParcelGrid {
    min_x: i32,
    min_z: i32,
    width: i32,
    parcel_size: i32,
}

impl PulseParcelGrid {
    /// Takes the server's full `ParcelEncoder` option set (1:1) so deployments can be configured
    /// straight from the instance's `appsettings`. `max_parcel_z` only bounds index *validity* on
    /// the server (`width * height`); the decode itself needs only `width`, so it's accepted but
    /// unused here — we trust server-issued indices.
    pub fn new(
        min_parcel_x: i32,
        min_parcel_z: i32,
        max_parcel_x: i32,
        _max_parcel_z: i32,
        padding: i32,
        parcel_size: i32,
    ) -> Self {
        let min_x = min_parcel_x - padding;
        let min_z = min_parcel_z - padding;
        let max_x = max_parcel_x + padding;
        let width = max_x - min_x + 1;
        Self {
            min_x,
            min_z,
            width,
            parcel_size,
        }
    }

    /// Inverse of the server's `ParcelEncoder.DecodeToGlobalPosition`. `local` is the in-parcel
    /// offset (DCL world convention; the `-z` render flip happens later in `update_player`).
    pub fn decode_to_world(&self, parcel_index: i32, local: Vec3) -> Vec3 {
        let x = parcel_index.rem_euclid(self.width) + self.min_x;
        let z = parcel_index.div_euclid(self.width) + self.min_z;
        Vec3::new(
            (x * self.parcel_size) as f32 + local.x,
            local.y,
            (z * self.parcel_size) as f32 + local.z,
        )
    }
}

impl Default for PulseParcelGrid {
    /// Server `appsettings.json` defaults (Genesis-City-sized bounding grid). Override per
    /// deployment with the target instance's `ParcelEncoder` section.
    fn default() -> Self {
        Self::new(-150, -150, 163, 158, 2, 16)
    }
}

/// Result of feeding one [`pulse::ServerMessage`] to the decoder. The Bevy glue wraps `Movement`
/// into a `PlayerUpdate { transport_id, address, message }`, applies `Joined`/`Left` to the alias
/// map, and transmits `Resync` reliably back to the server.
#[derive(Debug)]
pub enum PulseEvent {
    /// Handshake ack from the server. `success == false` carries the rejection reason.
    Connected { success: bool, error: Option<String> },
    /// Reconstructed full movement state for a subject, ready to push through the rfc4 pipeline.
    Movement { address: Address, movement: Box<rfc4::Movement> },
    /// Subject entered the interest set (or connected). Establishes the subject↔wallet alias.
    Joined { subject_id: u32, address: Address, profile_version: i32 },
    /// Subject left the interest set (or disconnected). Drop the alias / foreign player.
    Left { address: Address },
    /// Subject announced a new profile version.
    ProfileVersion { address: Address, version: i32 },
    /// A sequence gap was detected — transmit this reliably so the server replays full state.
    Resync(pulse::ResyncRequest),
}

/// Per-subject baseline. Pulse sends field-masked deltas against the last sequence we acked, so we
/// keep the last full [`pulse::PlayerState`] and overlay deltas onto it.
struct Subject {
    wallet: Address,
    last_seq: u32,
    baseline: pulse::PlayerState,
}

/// Owns all Pulse-specific sliding-window state. Single-threaded; lives inside the Pulse transport.
pub struct PulseDecoder {
    grid: PulseParcelGrid,
    subjects: HashMap<u32, Subject>,
}

impl PulseDecoder {
    pub fn new(grid: PulseParcelGrid) -> Self {
        Self {
            grid,
            subjects: HashMap::new(),
        }
    }

    /// Decode one server message, advancing per-subject state and emitting downstream events.
    pub fn handle(&mut self, msg: pulse::ServerMessage) -> Vec<PulseEvent> {
        use pulse::server_message::Message;

        let Some(message) = msg.message else {
            return Vec::new();
        };

        match message {
            Message::Handshake(h) => vec![PulseEvent::Connected {
                success: h.success,
                error: h.error,
            }],
            Message::PlayerJoined(j) => self.on_joined(j),
            Message::PlayerLeft(l) => self.on_left(l.subject_id),
            Message::PlayerStateFull(f) => self.on_full(f.subject_id, f.sequence, f.server_tick, f.state),
            // Teleport / emote start / stop all piggyback full state; treat them as a full refresh
            // so the subject's position never goes stale. The emote/teleport semantics themselves
            // are out of scope for the movement read path (handled later via the avatar pipeline).
            Message::Teleported(t) => self.on_full(t.subject_id, t.sequence, t.server_tick, t.state),
            Message::EmoteStarted(e) => self.on_full(e.subject_id, e.sequence, e.server_tick, e.player_state),
            Message::EmoteStopped(e) => self.on_full(e.subject_id, e.sequence, e.server_tick, e.player_state),
            Message::PlayerStateDelta(d) => self.on_delta(d),
            Message::PlayerProfileVersionAnnounced(p) => self.on_profile(p.subject_id, p.version),
        }
    }

    fn on_joined(&mut self, joined: pulse::PlayerJoined) -> Vec<PulseEvent> {
        let Some(full) = joined.state else {
            return Vec::new();
        };
        let Some(state) = full.state else {
            return Vec::new();
        };
        let Ok(address) = joined.user_id.parse::<Address>() else {
            bevy::log::warn!("pulse: PlayerJoined with unparseable user_id {:?}", joined.user_id);
            return Vec::new();
        };

        let movement = self.to_movement(&state, full.server_tick);
        self.subjects.insert(
            full.subject_id,
            Subject {
                wallet: address,
                last_seq: full.sequence,
                baseline: state,
            },
        );

        vec![
            PulseEvent::Joined {
                subject_id: full.subject_id,
                address,
                profile_version: joined.profile_version,
            },
            PulseEvent::Movement {
                address,
                movement: Box::new(movement),
            },
        ]
    }

    fn on_left(&mut self, subject_id: u32) -> Vec<PulseEvent> {
        match self.subjects.remove(&subject_id) {
            Some(subject) => vec![PulseEvent::Left { address: subject.wallet }],
            None => Vec::new(),
        }
    }

    /// Replace a known subject's baseline from a server-supplied full state. Unknown subjects are
    /// dropped: full state carries no wallet, so without a prior `PlayerJoined` we can't address it.
    fn on_full(
        &mut self,
        subject_id: u32,
        sequence: u32,
        server_tick: u32,
        state: Option<pulse::PlayerState>,
    ) -> Vec<PulseEvent> {
        let Some(state) = state else {
            return Vec::new();
        };
        let Some(subject) = self.subjects.get_mut(&subject_id) else {
            return Vec::new();
        };

        subject.baseline = state;
        subject.last_seq = sequence;
        let address = subject.wallet;
        let movement = self.to_movement_for(subject_id, server_tick);
        vec![PulseEvent::Movement {
            address,
            movement: Box::new(movement),
        }]
    }

    fn on_delta(&mut self, delta: pulse::PlayerStateDeltaTier0) -> Vec<PulseEvent> {
        let Some(subject) = self.subjects.get_mut(&delta.subject_id) else {
            // No baseline yet — ask the server for full state (known_seq 0 = "I have nothing").
            return vec![PulseEvent::Resync(pulse::ResyncRequest {
                subject_id: delta.subject_id,
                known_seq: 0,
            })];
        };

        // Already have this (e.g. a reliable resync retransmit of a seq we applied unreliably).
        if delta.new_seq <= subject.last_seq {
            return Vec::new();
        }

        // The delta is diffed from `baseline_seq`; we can only apply it if our state is exactly
        // that sequence. Otherwise we missed an intermediate delta — resync from what we have.
        if delta.baseline_seq != subject.last_seq {
            return vec![PulseEvent::Resync(pulse::ResyncRequest {
                subject_id: delta.subject_id,
                known_seq: subject.last_seq,
            })];
        }

        apply_delta(&mut subject.baseline, &delta);
        subject.last_seq = delta.new_seq;
        let address = subject.wallet;
        let movement = self.to_movement_for(delta.subject_id, delta.server_tick);
        vec![PulseEvent::Movement {
            address,
            movement: Box::new(movement),
        }]
    }

    fn on_profile(&self, subject_id: u32, version: i32) -> Vec<PulseEvent> {
        match self.subjects.get(&subject_id) {
            Some(subject) => vec![PulseEvent::ProfileVersion {
                address: subject.wallet,
                version,
            }],
            None => Vec::new(),
        }
    }

    /// Convenience: convert an already-stored subject baseline.
    fn to_movement_for(&self, subject_id: u32, server_tick: u32) -> rfc4::Movement {
        self.to_movement(&self.subjects[&subject_id].baseline, server_tick)
    }

    /// Reconstruct an `rfc4::Movement` from a full Pulse `PlayerState`. Position is parcel-decoded
    /// to world; `state_flags` is unpacked into the rfc4 bool fields; the animation rider is left
    /// empty (it arrives over LiveKit).
    fn to_movement(&self, state: &pulse::PlayerState, server_tick: u32) -> rfc4::Movement {
        let local = state.position.unwrap_or_default();
        let world = self
            .grid
            .decode_to_world(state.parcel_index, Vec3::new(local.x, local.y, local.z));
        let velocity = state.velocity.unwrap_or_default();
        let point_at = state.point_at.unwrap_or_default();
        let flags = state.state_flags;

        rfc4::Movement {
            // Pulse has no client-side send timestamp; the unified server clock (ms→s) is
            // monotonic per subject, which is what the interpolator needs.
            timestamp: server_tick as f32 / 1000.0,
            position_x: world.x,
            position_y: world.y,
            position_z: world.z,
            velocity_x: velocity.x,
            velocity_y: velocity.y,
            velocity_z: velocity.z,
            movement_blend_value: state.movement_blend,
            slide_blend_value: state.slide_blend,
            is_grounded: flag(flags, pulse::PlayerAnimationFlags::Grounded),
            // No Pulse equivalent: single jumps are inferred downstream from jump_count / velocity.
            is_jumping: false,
            is_long_jump: flag(flags, pulse::PlayerAnimationFlags::LongJump),
            is_long_fall: flag(flags, pulse::PlayerAnimationFlags::LongFall),
            is_falling: flag(flags, pulse::PlayerAnimationFlags::Falling),
            is_stunned: flag(flags, pulse::PlayerAnimationFlags::Stunned),
            rotation_y: state.rotation_y,
            is_emoting: false,
            jump_count: state.jump_count,
            // Pulse GlideState and rfc4 GlideState share identical enum values.
            glide_state: state.glide_state,
            point_at_x: point_at.x,
            point_at_y: point_at.y,
            point_at_z: point_at.z,
            is_pointing_at: flag(flags, pulse::PlayerAnimationFlags::PointingAt),
            head_ik_yaw_enabled: flag(flags, pulse::PlayerAnimationFlags::HeadYaw),
            head_ik_pitch_enabled: flag(flags, pulse::PlayerAnimationFlags::HeadPitch),
            head_yaw: state.head_yaw.unwrap_or(0.0),
            head_pitch: state.head_pitch.unwrap_or(0.0),
            scene_driven_animation: None,
        }
    }
}

fn flag(flags: u32, f: pulse::PlayerAnimationFlags) -> bool {
    flags & (f as u32) != 0
}

/// Overlay a field-masked delta onto a baseline full state. Present fields replace; absent fields
/// are carried forward. Position/velocity/head are quantized (dequantized via generated
/// accessors); discrete fields (parcel, flags, glide, jump) are plain.
fn apply_delta(baseline: &mut pulse::PlayerState, delta: &pulse::PlayerStateDeltaTier0) {
    if let Some(parcel) = delta.parcel_index {
        baseline.parcel_index = parcel;
    }

    {
        let position = baseline.position.get_or_insert_with(Default::default);
        if let Some(x) = delta.position_x_dequantized() {
            position.x = x;
        }
        if let Some(y) = delta.position_y_dequantized() {
            position.y = y;
        }
        if let Some(z) = delta.position_z_dequantized() {
            position.z = z;
        }
    }

    {
        let velocity = baseline.velocity.get_or_insert_with(Default::default);
        if let Some(x) = delta.velocity_x_dequantized() {
            velocity.x = x;
        }
        if let Some(y) = delta.velocity_y_dequantized() {
            velocity.y = y;
        }
        if let Some(z) = delta.velocity_z_dequantized() {
            velocity.z = z;
        }
    }

    if let Some(rotation_y) = delta.rotation_y_dequantized() {
        baseline.rotation_y = rotation_y;
    }
    if let Some(movement_blend) = delta.movement_blend_dequantized() {
        baseline.movement_blend = movement_blend;
    }
    if let Some(slide_blend) = delta.slide_blend_dequantized() {
        baseline.slide_blend = slide_blend;
    }
    if let Some(head_yaw) = delta.head_yaw_dequantized() {
        baseline.head_yaw = Some(head_yaw);
    }
    if let Some(head_pitch) = delta.head_pitch_dequantized() {
        baseline.head_pitch = Some(head_pitch);
    }
    if let Some(state_flags) = delta.state_flags {
        baseline.state_flags = state_flags;
    }
    if let Some(glide_state) = delta.glide_state {
        baseline.glide_state = glide_state;
    }
    if let Some(jump_count) = delta.jump_count {
        baseline.jump_count = jump_count;
    }

    if delta.point_at_x.is_some() || delta.point_at_y.is_some() || delta.point_at_z.is_some() {
        let point_at = baseline.point_at.get_or_insert_with(Default::default);
        if let Some(x) = delta.point_at_x_dequantized() {
            point_at.x = x;
        }
        if let Some(y) = delta.point_at_y_dequantized() {
            point_at.y = y;
        }
        if let Some(z) = delta.point_at_z_dequantized() {
            point_at.z = z;
        }
    }
}

#[cfg(test)]
mod test;
