//! What the player does once in world, for the funnel's "moved" and "playtime" questions:
//! "First Move In World" once per login, after a metre of grounded sideways movement, and
//! "time_spent_in_world" as the desktop client reports it (`time_spent` seconds, `realm_name`),
//! in chunks, so the sum per session is the playtime and a closed page loses at most one chunk.

use bevy::prelude::*;
use common::structs::{AvatarDynamicState, OutOfWorld, PrimaryUser};

use crate::{
    data_definition::{SegmentEvent, SegmentEventFirstMoveInWorld, SegmentEventTimeSpentInWorld},
    segment_system::{SegmentConfig, SegmentMetricsEvents},
};

pub const TIME_SPENT_CHUNK_SECS: f32 = 30.0;
pub const FIRST_MOVE_METRES: f32 = 1.0;

#[derive(Default)]
pub struct TimeSpent {
    secs: f32,
    realm: Option<String>,
    address: Option<String>,
}

impl TimeSpent {
    /// A frame of `dt` seconds; `in_world` is false behind the loading screen. The chunk closes
    /// when it reaches [`TIME_SPENT_CHUNK_SECS`] or when the realm or the login changes.
    pub fn tick(
        &mut self,
        in_world: bool,
        realm: &str,
        address: &str,
        dt: f32,
    ) -> Option<SegmentEventTimeSpentInWorld> {
        let changed = self.realm.as_deref().is_some_and(|r| r != realm)
            || self.address.as_deref().is_some_and(|a| a != address);
        let closed = if changed || self.secs >= TIME_SPENT_CHUNK_SECS {
            self.close()
        } else {
            None
        };
        if realm != "no-realm" && address != "unauthenticated" {
            self.realm = Some(realm.to_owned());
            self.address = Some(address.to_owned());
            if in_world {
                self.secs += dt;
            }
        }
        closed
    }

    fn close(&mut self) -> Option<SegmentEventTimeSpentInWorld> {
        let realm_name = self.realm.take()?;
        self.address = None;
        let time_spent = std::mem::take(&mut self.secs);
        (time_spent > 0.0).then_some(SegmentEventTimeSpentInWorld {
            time_spent,
            realm_name,
        })
    }
}

/// Grounded sideways distance per login: one frame of a landing resolved against a slope moves
/// the player centimetres, walking crosses [`FIRST_MOVE_METRES`] in under a second.
#[derive(Default)]
pub struct FirstMove {
    address: Option<String>,
    metres: f32,
    sent: bool,
}

impl FirstMove {
    pub fn tick(&mut self, address: &str, grounded_speed: f32, dt: f32) -> bool {
        if self.address.as_deref() != Some(address) {
            self.address = Some(address.to_owned());
            self.metres = 0.0;
            self.sent = false;
        }
        if self.sent || address == "unauthenticated" {
            return false;
        }
        self.metres += grounded_speed * dt;
        self.sent = self.metres >= FIRST_MOVE_METRES;
        self.sent
    }
}

#[allow(clippy::type_complexity)]
pub fn report_in_world(
    player: Query<(&AvatarDynamicState, Has<OutOfWorld>), With<PrimaryUser>>,
    time: Res<Time>,
    config: Option<Res<SegmentConfig>>,
    metrics: Option<ResMut<SegmentMetricsEvents>>,
    mut spent: Local<TimeSpent>,
    mut first_move: Local<FirstMove>,
) {
    let (Some(config), Some(mut metrics)) = (config, metrics) else {
        return;
    };
    let common = config.common();
    let player = player.single().ok();
    let in_world = player.is_some_and(|(_, oow)| !oow);
    let dt = time.delta_secs();

    if let Some(event) = spent.tick(in_world, &common.realm, &common.dcl_eth_address, dt) {
        metrics.add_event(SegmentEvent::TimeSpentInWorld(event));
    }

    let grounded_speed = match player {
        Some((dynamics, false)) if dynamics.ground_height <= 0.2 => dynamics.velocity.xz().length(),
        _ => 0.0,
    };
    if first_move.tick(&common.dcl_eth_address, grounded_speed, dt) {
        metrics.add_event(SegmentEvent::FirstMoveInWorld(
            SegmentEventFirstMoveInWorld {},
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // What the warehouse sums: only time in world, closed per chunk and under the realm it was
    // spent in when the realm changes.
    #[test]
    fn time_spent_closes_per_chunk_and_on_realm_change() {
        let mut spent = TimeSpent::default();
        assert!(spent
            .tick(true, "no-realm", "unauthenticated", 5.0)
            .is_none());
        assert!(spent.tick(false, "main", "0xa", 5.0).is_none());
        for _ in 0..30 {
            assert!(spent.tick(true, "main", "0xa", 1.0).is_none());
        }
        let chunk = spent.tick(true, "main", "0xa", 1.0).unwrap();
        assert_eq!(chunk.realm_name, "main");
        assert_eq!(chunk.time_spent, 30.0);

        spent.tick(true, "main", "0xa", 2.5);
        let chunk = spent.tick(true, "world.dcl.eth", "0xa", 1.0).unwrap();
        assert_eq!(chunk.realm_name, "main");
        assert_eq!(chunk.time_spent, 3.5);
        assert!(spent.tick(true, "world.dcl.eth", "0xa", 1.0).is_none());
    }

    // A landing resolved against a slope is one fast frame; walking is many.
    #[test]
    fn first_move_needs_a_metre_of_walking_per_login() {
        let mut first = FirstMove::default();
        assert!(!first.tick("unauthenticated", 2.0, 1.0));
        assert!(!first.tick("0xa", 12.0, 1.0 / 60.0));
        assert!(!first.tick("0xa", 2.0, 0.25));
        assert!(first.tick("0xa", 2.0, 0.25));
        assert!(!first.tick("0xa", 2.0, 1.0));
        assert!(first.tick("0xb", 2.0, 1.0));
    }
}
