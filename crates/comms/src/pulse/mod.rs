//! Pulse transport — read side.
//!
//! Pulse is the server-authoritative realtime movement backend (UDP/ENet). This module owns the
//! *decode + per-subject sliding-window state* and reconstructs each subject's full state into an
//! [`rfc4::Movement`], so that everything downstream (`global_crdt::update_player`,
//! `foreign_dynamics` interpolation, scene-driven-animation resolution) is reused verbatim — the
//! rest of the client never learns Pulse exists below the `PlayerUpdate` line.
//!
//! Two contracts are agreed out-of-band, not on the wire:
//!   * the quantization ABI (linear `{min,max,bits}` or power-law `{max,pow,bits}` per delta field)
//!     — handled by the `*_dequantized()` accessors generated from the proto descriptor in
//!     `dcl_component`;
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

    /// Inverse of [`Self::decode_to_world`]: a world position (DCL convention) → the server's
    /// `parcel_index` plus the in-parcel local offset, matching the server's `ParcelEncoder.Encode`
    /// + relative-position split. Used to build our own `TeleportRequest`.
    pub fn encode_to_parcel(&self, world: Vec3) -> (i32, Vec3) {
        let size = self.parcel_size as f32;
        let parcel_x = (world.x / size).floor() as i32;
        let parcel_z = (world.z / size).floor() as i32;
        let parcel_index = (parcel_x - self.min_x) + (parcel_z - self.min_z) * self.width;
        let local = Vec3::new(
            world.x - (parcel_x * self.parcel_size) as f32,
            world.y,
            world.z - (parcel_z * self.parcel_size) as f32,
        );
        (parcel_index, local)
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
    Connected {
        success: bool,
        error: Option<String>,
    },
    /// Reconstructed full movement state for a subject, ready to push through the rfc4 pipeline.
    /// `teleport` marks state from a `TeleportPerformed` — foreign dynamics snaps to it instead of
    /// interpolating, since it represents a discontinuous reposition, not travel.
    Movement {
        address: Address,
        movement: Box<rfc4::Movement>,
        teleport: bool,
    },
    /// Subject entered the interest set (or connected). Establishes the subject↔wallet alias.
    Joined {
        subject_id: u32,
        address: Address,
        profile_version: i32,
    },
    /// Subject left the interest set (or disconnected). Drop the alias / foreign player.
    Left { address: Address },
    /// Subject announced a new profile version.
    ProfileVersion { address: Address, version: i32 },
    /// Subject started an emote. Emitted alongside the piggybacked `Movement`. `tick` is the
    /// server tick, used downstream only as a monotonic id so re-triggering the same urn replays.
    EmoteStart {
        address: Address,
        urn: String,
        tick: u32,
    },
    /// Subject's emote stopped (one-shot completed or looping cancelled).
    EmoteStop { address: Address },
    /// A sequence gap was detected — transmit this reliably so the server replays full state.
    Resync(pulse::ResyncRequest),
}

/// Borrowed context handed to `Broadcast::to_pulse` so an outbound message can build its Pulse frame:
/// the parcel grid (to encode world position) and the last `PlayerState` we sent (movement caches it
/// here so an emote can attach the full state the server requires on an `EmoteStart`).
pub struct PulseCtx<'a> {
    pub grid: &'a PulseParcelGrid,
    pub last_state: &'a mut Option<pulse::PlayerState>,
}

/// Per-subject baseline. Pulse sends field-masked deltas against the last sequence we acked, so we
/// keep the last reconstructed full state and overlay deltas onto it.
struct Subject {
    wallet: Address,
    last_seq: u32,
    baseline: SubjectState,
}

/// A subject's reconstructed full state, in floats. Both full [`pulse::PlayerState`] snapshots and
/// field-masked deltas dequantize into this — each field via its own proto-derived accessor — so the
/// baseline is independent of the wire quantization grid (a full state and a delta needn't share
/// identical `{min,max,bits}` for a field). `to_movement` then reads it directly.
#[derive(Debug, Clone, Default)]
struct SubjectState {
    parcel_index: i32,
    /// In-parcel local position (world altitude in `y`); parcel-decoded to world in `to_movement`.
    position: Vec3,
    velocity: Vec3,
    rotation_y: f32,
    movement_blend: f32,
    slide_blend: f32,
    head_yaw: Option<f32>,
    head_pitch: Option<f32>,
    state_flags: u32,
    /// Raw `GlideState` discriminant (shared verbatim with rfc4's identical enum).
    glide_state: i32,
    jump_count: i32,
    /// Absolute world point-at target; only meaningful when `POINTING_AT` is set in `state_flags`.
    point_at: Vec3,
}

impl SubjectState {
    /// Dequantize a wire full state into the float baseline.
    fn from_player_state(s: &pulse::PlayerState) -> Self {
        Self {
            parcel_index: s.parcel_index,
            position: Vec3::new(
                s.position_x_dequantized(),
                s.position_y_dequantized(),
                s.position_z_dequantized(),
            ),
            velocity: Vec3::new(
                s.velocity_x_dequantized(),
                s.velocity_y_dequantized(),
                s.velocity_z_dequantized(),
            ),
            rotation_y: s.rotation_y_dequantized(),
            movement_blend: s.movement_blend_dequantized(),
            slide_blend: s.slide_blend_dequantized(),
            head_yaw: s.head_yaw_dequantized(),
            head_pitch: s.head_pitch_dequantized(),
            state_flags: s.state_flags,
            glide_state: s.glide_state,
            jump_count: s.jump_count,
            point_at: Vec3::new(
                s.point_at_x_dequantized().unwrap_or_default(),
                s.point_at_y_dequantized().unwrap_or_default(),
                s.point_at_z_dequantized().unwrap_or_default(),
            ),
        }
    }

    /// Overlay a field-masked delta. Present fields replace (dequantized via the delta's own
    /// accessors); absent fields carry forward. Discrete fields (parcel, flags, glide, jump) are
    /// plain; position/velocity/head/point-at are quantized.
    fn apply_delta(&mut self, delta: &pulse::PlayerStateDeltaTier0) {
        if let Some(parcel) = delta.parcel_index {
            self.parcel_index = parcel;
        }
        if let Some(x) = delta.position_x_dequantized() {
            self.position.x = x;
        }
        if let Some(y) = delta.position_y_dequantized() {
            self.position.y = y;
        }
        if let Some(z) = delta.position_z_dequantized() {
            self.position.z = z;
        }
        if let Some(x) = delta.velocity_x_dequantized() {
            self.velocity.x = x;
        }
        if let Some(y) = delta.velocity_y_dequantized() {
            self.velocity.y = y;
        }
        if let Some(z) = delta.velocity_z_dequantized() {
            self.velocity.z = z;
        }
        if let Some(rotation_y) = delta.rotation_y_dequantized() {
            self.rotation_y = rotation_y;
        }
        if let Some(movement_blend) = delta.movement_blend_dequantized() {
            self.movement_blend = movement_blend;
        }
        if let Some(slide_blend) = delta.slide_blend_dequantized() {
            self.slide_blend = slide_blend;
        }
        if let Some(head_yaw) = delta.head_yaw_dequantized() {
            self.head_yaw = Some(head_yaw);
        }
        if let Some(head_pitch) = delta.head_pitch_dequantized() {
            self.head_pitch = Some(head_pitch);
        }
        if let Some(state_flags) = delta.state_flags {
            self.state_flags = state_flags;
        }
        if let Some(glide_state) = delta.glide_state {
            self.glide_state = glide_state;
        }
        if let Some(jump_count) = delta.jump_count {
            self.jump_count = jump_count;
        }
        if let Some(x) = delta.point_at_x_dequantized() {
            self.point_at.x = x;
        }
        if let Some(y) = delta.point_at_y_dequantized() {
            self.point_at.y = y;
        }
        if let Some(z) = delta.point_at_z_dequantized() {
            self.point_at.z = z;
        }
    }
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
            Message::PlayerStateFull(f) => {
                self.on_full(f.subject_id, f.sequence, f.server_tick, f.state, false)
            }
            // Teleport / emote start / stop all piggyback full state; treat them as a full refresh
            // so the subject's position never goes stale, then (for emotes) emit the emote event so
            // the avatar pipeline plays/stops it. Order: movement first so the position is current
            // before the emote starts. Teleport is flagged so foreign dynamics snaps rather than
            // interpolates across the jump.
            Message::Teleported(t) => {
                self.on_full(t.subject_id, t.sequence, t.server_tick, t.state, true)
            }
            Message::EmoteStarted(e) => {
                let mut events = self.on_full(
                    e.subject_id,
                    e.sequence,
                    e.server_tick,
                    e.player_state,
                    false,
                );
                if let Some(subject) = self.subjects.get(&e.subject_id) {
                    events.push(PulseEvent::EmoteStart {
                        address: subject.wallet,
                        urn: e.emote_id,
                        tick: e.server_tick,
                    });
                }
                events
            }
            Message::EmoteStopped(e) => {
                let mut events = self.on_full(
                    e.subject_id,
                    e.sequence,
                    e.server_tick,
                    e.player_state,
                    false,
                );
                if let Some(subject) = self.subjects.get(&e.subject_id) {
                    events.push(PulseEvent::EmoteStop {
                        address: subject.wallet,
                    });
                }
                events
            }
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
            bevy::log::warn!(
                "pulse: PlayerJoined with unparseable user_id {:?}",
                joined.user_id
            );
            return Vec::new();
        };

        let baseline = SubjectState::from_player_state(&state);
        let movement = self.to_movement(&baseline, full.server_tick);
        self.subjects.insert(
            full.subject_id,
            Subject {
                wallet: address,
                last_seq: full.sequence,
                baseline,
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
                teleport: false,
            },
        ]
    }

    fn on_left(&mut self, subject_id: u32) -> Vec<PulseEvent> {
        match self.subjects.remove(&subject_id) {
            Some(subject) => vec![PulseEvent::Left {
                address: subject.wallet,
            }],
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
        teleport: bool,
    ) -> Vec<PulseEvent> {
        let Some(state) = state else {
            return Vec::new();
        };
        let Some(subject) = self.subjects.get_mut(&subject_id) else {
            return Vec::new();
        };

        subject.baseline = SubjectState::from_player_state(&state);
        subject.last_seq = sequence;
        let address = subject.wallet;
        let movement = self.to_movement_for(subject_id, server_tick);
        vec![PulseEvent::Movement {
            address,
            movement: Box::new(movement),
            teleport,
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

        subject.baseline.apply_delta(&delta);
        subject.last_seq = delta.new_seq;
        let address = subject.wallet;
        let mut movement = self.to_movement_for(delta.subject_id, delta.server_tick);
        // A delta carries position quantized to the parcel grid, so the true position is only known
        // to within ±half a step. Hand that box to the interpolator (via the rfc4 carrier) so it
        // dead-reckons inside the box instead of snapping to the quantized centre every packet.
        // Full-state / join movements leave this unset (exact position).
        movement.position_precision = Some(rfc4::PositionPrecision {
            x: pulse::PlayerStateDeltaTier0::position_x_step() * 0.5,
            y: pulse::PlayerStateDeltaTier0::position_y_step() * 0.5,
            z: pulse::PlayerStateDeltaTier0::position_z_step() * 0.5,
        });
        vec![PulseEvent::Movement {
            address,
            movement: Box::new(movement),
            teleport: false,
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

    /// Reconstruct an `rfc4::Movement` from a subject's float baseline. Position is parcel-decoded
    /// to world; `state_flags` is unpacked into the rfc4 bool fields; the animation rider is left
    /// empty (it arrives over LiveKit).
    fn to_movement(&self, state: &SubjectState, server_tick: u32) -> rfc4::Movement {
        let world = self
            .grid
            .decode_to_world(state.parcel_index, state.position);
        let velocity = state.velocity;
        let point_at = state.point_at;
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
            // Head angles are quantized over the unsigned [0, 360] range, so the sender wraps
            // negatives (via `rem_euclid`); undo that to the signed range the head-IK consumer
            // expects (pitch especially — it's clamped to ±cone, not wrapped).
            head_yaw: state.head_yaw.map(signed_angle).unwrap_or(0.0),
            head_pitch: state.head_pitch.map(signed_angle).unwrap_or(0.0),
            scene_driven_animation: None,
            // Stamped only on delta-derived movements (see `on_delta`); a full state carries an
            // exact position, so the shared builder leaves the box unset (precise).
            position_precision: None,
        }
    }
}

fn flag(flags: u32, f: pulse::PlayerAnimationFlags) -> bool {
    flags & (f as u32) != 0
}

/// Map a dequantized head angle from the wire's unsigned [0, 360) range back to the signed
/// (-180, 180] range the head-IK consumer works in. Inverse of the sender's `rem_euclid(360)`.
fn signed_angle(deg: f32) -> f32 {
    if deg > 180.0 {
        deg - 360.0
    } else {
        deg
    }
}

/// Inverse of [`PulseDecoder::to_movement`]: pack a locally-built [`rfc4::Movement`] into the Pulse
/// [`PlayerState`] we send. World position is split into `parcel_index` + in-parcel local; the rfc4
/// bool fields are folded back into `state_flags`; head yaw/pitch and point-at ride only when their
/// enable flag is set (matching Unity's `WritePlayerState`). Fields with no Pulse equivalent
/// (`is_jumping`, `is_emoting`, `scene_driven_animation`) are dropped — animation stays on LiveKit.
pub(crate) fn from_movement(
    movement: &rfc4::Movement,
    grid: &PulseParcelGrid,
) -> pulse::PlayerState {
    let world = Vec3::new(
        movement.position_x,
        movement.position_y,
        movement.position_z,
    );
    let (parcel_index, local) = grid.encode_to_parcel(world);

    let mut state_flags = 0u32;
    let mut set = |on: bool, f: pulse::PlayerAnimationFlags| {
        if on {
            state_flags |= f as u32;
        }
    };
    set(movement.is_grounded, pulse::PlayerAnimationFlags::Grounded);
    set(movement.is_long_jump, pulse::PlayerAnimationFlags::LongJump);
    set(movement.is_long_fall, pulse::PlayerAnimationFlags::LongFall);
    set(movement.is_falling, pulse::PlayerAnimationFlags::Falling);
    set(movement.is_stunned, pulse::PlayerAnimationFlags::Stunned);
    set(
        movement.head_ik_yaw_enabled,
        pulse::PlayerAnimationFlags::HeadYaw,
    );
    set(
        movement.head_ik_pitch_enabled,
        pulse::PlayerAnimationFlags::HeadPitch,
    );
    set(
        movement.is_pointing_at,
        pulse::PlayerAnimationFlags::PointingAt,
    );

    use pulse::PlayerState as P;
    pulse::PlayerState {
        parcel_index,
        position_x: P::position_x_quantized(local.x),
        position_y: P::position_y_quantized(local.y),
        position_z: P::position_z_quantized(local.z),
        velocity_x: P::velocity_x_quantized(movement.velocity_x),
        velocity_y: P::velocity_y_quantized(movement.velocity_y),
        velocity_z: P::velocity_z_quantized(movement.velocity_z),
        rotation_y: P::rotation_y_quantized(movement.rotation_y),
        movement_blend: P::movement_blend_quantized(movement.movement_blend_value),
        slide_blend: P::slide_blend_quantized(movement.slide_blend_value),
        // Head angles quantize over the unsigned [0, 360] range; wrap negatives so a left/up look
        // (negative yaw/pitch) doesn't clamp to 0. The receiver undoes this via `signed_angle`.
        head_yaw: movement
            .head_ik_yaw_enabled
            .then(|| P::head_yaw_quantized(movement.head_yaw.rem_euclid(360.0))),
        head_pitch: movement
            .head_ik_pitch_enabled
            .then(|| P::head_pitch_quantized(movement.head_pitch.rem_euclid(360.0))),
        state_flags,
        glide_state: movement.glide_state,
        jump_count: movement.jump_count,
        point_at_x: movement
            .is_pointing_at
            .then(|| P::point_at_x_quantized(movement.point_at_x)),
        point_at_y: movement
            .is_pointing_at
            .then(|| P::point_at_y_quantized(movement.point_at_y)),
        point_at_z: movement
            .is_pointing_at
            .then(|| P::point_at_z_quantized(movement.point_at_z)),
    }
}

#[cfg(test)]
mod test;
