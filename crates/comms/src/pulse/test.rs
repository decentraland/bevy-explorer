use alloy_core::primitives::Address;
use dcl_component::proto_components::pulse;

use super::*;

const SUBJECT: u32 = 42;
const WALLET: &str = "0x0000000000000000000000000000000000000001";

// Parcel (10, 20) under the default grid: min_x = min_z = -150 - 2 = -152, width = 318.
// index = (10 - -152) + (20 - -152) * 318 = 162 + 172*318 = 54858. World base = (160, _, 320).
const PARCEL_INDEX: i32 = 54858;

fn wallet() -> Address {
    WALLET.parse().unwrap()
}

fn server_msg(message: pulse::server_message::Message) -> pulse::ServerMessage {
    pulse::ServerMessage {
        message: Some(message),
    }
}

fn player_state(local: (f32, f32, f32), flags: u32) -> pulse::PlayerState {
    pulse::PlayerState {
        parcel_index: PARCEL_INDEX,
        position_x: pulse::PlayerState::position_x_quantized(local.0),
        position_y: pulse::PlayerState::position_y_quantized(local.1),
        position_z: pulse::PlayerState::position_z_quantized(local.2),
        state_flags: flags,
        ..Default::default()
    }
}

fn joined(sequence: u32, local: (f32, f32, f32), flags: u32) -> pulse::server_message::Message {
    pulse::server_message::Message::PlayerJoined(pulse::PlayerJoined {
        user_id: WALLET.to_string(),
        profile_version: 7,
        state: Some(pulse::PlayerStateFull {
            subject_id: SUBJECT,
            sequence,
            server_tick: 1000,
            state: Some(player_state(local, flags)),
        }),
        // announced by the server since protocol #31; the decoder does not read it yet
        realm: String::new(),
    })
}

fn only_movement(events: Vec<PulseEvent>) -> rfc4::Movement {
    let mut found = None;
    for event in events {
        if let PulseEvent::Movement { movement, .. } = event {
            assert!(found.is_none(), "expected exactly one Movement event");
            found = Some(*movement);
        }
    }
    found.expect("no Movement event")
}

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 0.05, "expected ~{b}, got {a}");
}

fn movement_timestamps(events: Vec<PulseEvent>) -> Vec<f64> {
    events
        .into_iter()
        .filter_map(|event| match event {
            PulseEvent::Movement { timestamp, .. } => Some(timestamp),
            _ => None,
        })
        .collect()
}

/// Real ticks captured from a live session, where a peer's stop went missing. They are milliseconds
/// and only ~100ms apart at ~2.35e9, which is past the point where an `f32` can tell them apart:
/// all three collapse onto 2350530.75. The receiver drops any update that is not strictly newer
/// than the last applied one, so the deltas that decelerated and then zeroed the peer's velocity
/// were discarded, and the avatar kept dead-reckoning at its last-known speed indefinitely.
#[test]
fn consecutive_ticks_stay_distinct_at_realistic_magnitudes() {
    const TICKS: [u32; 3] = [2350530722, 2350530821, 2350530940];

    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), 0)));

    let stamps: Vec<f64> = TICKS
        .iter()
        .enumerate()
        .flat_map(|(i, tick)| {
            let delta = pulse::PlayerStateDeltaTier0 {
                subject_id: SUBJECT,
                baseline_seq: 5 + i as u32,
                new_seq: 6 + i as u32,
                server_tick: *tick,
                position_x: Some(128 + i as u32),
                ..Default::default()
            };
            movement_timestamps(decoder.handle(server_msg(
                pulse::server_message::Message::PlayerStateDelta(delta),
            )))
        })
        .collect();

    assert_eq!(
        stamps.len(),
        TICKS.len(),
        "every delta should emit movement"
    );
    for (a, b) in stamps.iter().zip(stamps.iter().skip(1)) {
        assert!(
            b > a,
            "ticks must stay strictly increasing after conversion, got {a} then {b}"
        );
    }
    // and the gaps must survive intact, not just be non-zero
    assert!((stamps[1] - stamps[0] - 0.099).abs() < 1e-6);
    assert!((stamps[2] - stamps[1] - 0.119).abs() < 1e-6);
}

#[test]
fn parcel_decode_matches_server_scheme() {
    let grid = PulseParcelGrid::default();
    let world = grid.decode_to_world(PARCEL_INDEX, Vec3::new(1.0, 2.0, 3.0));
    approx(world.x, 161.0); // 10*16 + 1
    approx(world.y, 2.0);
    approx(world.z, 323.0); // 20*16 + 3
}

#[test]
fn join_emits_alias_and_world_movement() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    let grounded = pulse::PlayerAnimationFlags::Grounded as u32;
    let events = decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), grounded)));

    let join = events
        .iter()
        .find_map(|e| match e {
            PulseEvent::Joined {
                subject_id,
                address,
                profile_version,
            } => Some((*subject_id, *address, *profile_version)),
            _ => None,
        })
        .expect("no Joined event");
    assert_eq!(join, (SUBJECT, wallet(), 7));

    let movement = only_movement(events);
    approx(movement.position_x, 161.0);
    approx(movement.position_z, 323.0);
    assert!(movement.is_grounded);
    assert!(movement.scene_driven_animation.is_none());
}

#[test]
fn in_sequence_delta_applies_and_dequantizes() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), 0)));

    // position_x is local, 8 bits over [0,16]; encoded 128 → 128/255*16 ≈ 8.031.
    let delta = pulse::PlayerStateDeltaTier0 {
        subject_id: SUBJECT,
        baseline_seq: 5,
        new_seq: 6,
        server_tick: 1033,
        position_x: Some(128),
        ..Default::default()
    };
    let movement = only_movement(decoder.handle(server_msg(
        pulse::server_message::Message::PlayerStateDelta(delta),
    )));

    // local x replaced (8.031), y/z carried forward from the join (2, 3); parcel base (160, 320).
    approx(movement.position_x, 168.031);
    approx(movement.position_y, 2.0);
    approx(movement.position_z, 323.0);
}

#[test]
fn head_angles_round_trip_signed() {
    // A left/up look: negative yaw and pitch. Head angles quantize over the unsigned [0, 360]
    // range, so without the sender's `rem_euclid` wrap + receiver's `signed_angle` unwrap these
    // would clamp to 0 — only right/down (positive) would survive.
    let grid = PulseParcelGrid::default();
    let movement = rfc4::Movement {
        position_x: 161.0,
        position_y: 2.0,
        position_z: 323.0,
        head_ik_yaw_enabled: true,
        head_ik_pitch_enabled: true,
        head_yaw: -30.0,
        head_pitch: -20.0,
        ..Default::default()
    };
    let state = from_movement(&movement, &grid);

    let mut decoder = PulseDecoder::new(grid);
    let out = only_movement(decoder.handle(server_msg(
        pulse::server_message::Message::PlayerJoined(pulse::PlayerJoined {
            user_id: WALLET.to_string(),
            profile_version: 1,
            state: Some(pulse::PlayerStateFull {
                subject_id: SUBJECT,
                sequence: 1,
                server_tick: 1000,
                state: Some(state),
            }),
            realm: String::new(),
        }),
    )));

    assert!(out.head_ik_yaw_enabled && out.head_ik_pitch_enabled);
    // 7 bits over 360° ≈ 2.83°/step.
    assert!((out.head_yaw - (-30.0)).abs() < 3.0, "yaw {}", out.head_yaw);
    assert!(
        (out.head_pitch - (-20.0)).abs() < 3.0,
        "pitch {}",
        out.head_pitch
    );
}

#[test]
fn sequence_gap_requests_resync_without_applying() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), 0)));

    // baseline_seq 4 ≠ our last_seq 5 → we missed an intermediate delta.
    let delta = pulse::PlayerStateDeltaTier0 {
        subject_id: SUBJECT,
        baseline_seq: 4,
        new_seq: 6,
        position_x: Some(200),
        ..Default::default()
    };
    let events = decoder.handle(server_msg(
        pulse::server_message::Message::PlayerStateDelta(delta),
    ));

    assert_eq!(events.len(), 1);
    match &events[0] {
        PulseEvent::Resync(r) => {
            assert_eq!(r.subject_id, SUBJECT);
            assert_eq!(r.known_seq, 5);
        }
        other => panic!("expected Resync, got {other:?}"),
    }
}

#[test]
fn stale_delta_is_dropped() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), 0)));

    // new_seq 5 == last_seq 5 → already applied.
    let delta = pulse::PlayerStateDeltaTier0 {
        subject_id: SUBJECT,
        baseline_seq: 4,
        new_seq: 5,
        ..Default::default()
    };
    assert!(decoder
        .handle(server_msg(
            pulse::server_message::Message::PlayerStateDelta(delta)
        ))
        .is_empty());
}

#[test]
fn delta_for_unknown_subject_requests_full() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    let delta = pulse::PlayerStateDeltaTier0 {
        subject_id: 99,
        baseline_seq: 0,
        new_seq: 1,
        ..Default::default()
    };
    let events = decoder.handle(server_msg(
        pulse::server_message::Message::PlayerStateDelta(delta),
    ));
    match &events[0] {
        PulseEvent::Resync(r) => {
            assert_eq!(r.subject_id, 99);
            assert_eq!(r.known_seq, 0);
        }
        other => panic!("expected Resync, got {other:?}"),
    }
}

#[test]
fn left_drops_subject_and_emits_address() {
    let mut decoder = PulseDecoder::new(PulseParcelGrid::default());
    decoder.handle(server_msg(joined(5, (1.0, 2.0, 3.0), 0)));

    let events = decoder.handle(server_msg(pulse::server_message::Message::PlayerLeft(
        pulse::PlayerLeft {
            subject_id: SUBJECT,
        },
    )));
    match &events[0] {
        PulseEvent::Left { address } => assert_eq!(*address, wallet()),
        other => panic!("expected Left, got {other:?}"),
    }

    // After leaving, a delta should be treated as unknown (resync), proving the baseline is gone.
    let delta = pulse::PlayerStateDeltaTier0 {
        subject_id: SUBJECT,
        baseline_seq: 5,
        new_seq: 6,
        ..Default::default()
    };
    assert!(matches!(
        decoder
            .handle(server_msg(
                pulse::server_message::Message::PlayerStateDelta(delta)
            ))
            .as_slice(),
        [PulseEvent::Resync(_)]
    ));
}
