//! The loading screen as mobile reports it (`loading_screen.gd`, `explorer.gd`): "Screen Viewed"
//! LOADING_START when it shows, LOADING_END when it hides, LOADING_TIMEOUT once it has been up
//! for [`LOADING_TIMEOUT_SECS`]. Shown by the screen's own rule (`system_ui/oow.rs`): a signed-in
//! player out of world and not held for a lobby. How it ended comes from the system that lets the
//! player back in (`ReturnedToWorld`, restricted_actions' `handle_out_of_world`).

use bevy::prelude::*;
use common::structs::{OutOfWorld, PrimaryUser, ReturnedToWorld, WorldHold};
use wallet::Wallet;

use crate::{
    data_definition::{SegmentEvent, SegmentEventScreenViewed},
    primary_player_parcel_position::parcel_of,
    segment_system::SegmentMetricsEvents,
};

pub const LOADING_TIMEOUT_SECS: f32 = 90.0;

#[derive(Default)]
pub struct LoadingScreen {
    /// The showing under way: for how long, whether it already timed out.
    open: Option<(f32, bool)>,
    /// The realm the last showing loaded; none before the first.
    realm: Option<String>,
    /// How the player was last let back in, until the screen's hiding reports it.
    returned: Option<ReturnedToWorld>,
}

impl LoadingScreen {
    /// A frame of `dt` seconds. `when` is mobile's: the first load of the run, a load into
    /// another realm, a move within it.
    pub fn tick(
        &mut self,
        visible: bool,
        returned: Option<ReturnedToWorld>,
        realm: &str,
        position: &str,
        dt: f32,
    ) -> Vec<SegmentEventScreenViewed> {
        if returned.is_some() {
            self.returned = returned;
        }
        let mut out = Vec::new();
        match (&mut self.open, visible) {
            (None, true) => {
                let when = match self.realm.as_deref() {
                    None => "on_explorer_ready",
                    Some(last) if last != realm => "on_changerealm",
                    Some(_) => "on_moveto",
                };
                self.realm = Some(realm.to_owned());
                out.push(SegmentEventScreenViewed::new(
                    "LOADING_START",
                    serde_json::json!({ "position": position, "realm": realm, "when": when }),
                ));
                self.open = Some((0.0, false));
            }
            (Some((elapsed, timed_out)), true) => {
                *elapsed += dt;
                if !*timed_out && *elapsed >= LOADING_TIMEOUT_SECS {
                    *timed_out = true;
                    out.push(SegmentEventScreenViewed::new(
                        "LOADING_TIMEOUT",
                        serde_json::json!({
                            "timeout_reason": "max_time",
                            "total_elapsed_seconds": LOADING_TIMEOUT_SECS as u32,
                        }),
                    ));
                }
            }
            (Some(_), false) => {
                self.open = None;
                let mut end = serde_json::json!({ "position": position, "status": "Success" });
                let error = match self.returned.take() {
                    Some(ReturnedToWorld::SceneBroken) => Some("scene_broken"),
                    Some(ReturnedToWorld::SceneLoadFailed) => Some("scene_load_failed"),
                    Some(ReturnedToWorld::SceneReady) | None => None,
                };
                if let Some(error) = error {
                    end["status"] = "Failed".into();
                    end["error"] = error.into();
                }
                out.push(SegmentEventScreenViewed::new("LOADING_END", end));
            }
            (None, false) => {}
        }
        out
    }
}

#[allow(clippy::too_many_arguments)]
pub fn report_loading_screen(
    player: Query<(&Transform, Has<OutOfWorld>), With<PrimaryUser>>,
    wallet: Option<Res<Wallet>>,
    world_hold: Option<Res<WorldHold>>,
    time: Res<Time>,
    config: Option<Res<crate::segment_system::SegmentConfig>>,
    metrics: Option<ResMut<SegmentMetricsEvents>>,
    mut returned: EventReader<ReturnedToWorld>,
    mut screen: Local<LoadingScreen>,
) {
    let (Some(config), Some(mut metrics)) = (config, metrics) else {
        return;
    };
    let signed_in = wallet.as_ref().is_some_and(|w| w.address().is_some());
    let (position, oow) = match player.single() {
        Ok((transform, oow)) => {
            let parcel = parcel_of(transform.translation);
            (format!("{},{}", parcel.x, parcel.y), oow)
        }
        Err(_) => ("no-position".to_owned(), false),
    };
    let visible = signed_in && oow && world_hold.is_none();
    for event in screen.tick(
        visible,
        returned.read().last().copied(),
        &config.common().realm,
        &position,
        time.delta_secs(),
    ) {
        metrics.add_event(SegmentEvent::ScreenViewed(event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // keys sorted: serde_json's `preserve_order` is unified in by deno_core in the full
    // workspace build, so the raw string's key order depends on what else is being built
    fn names(events: &[SegmentEventScreenViewed]) -> Vec<String> {
        events
            .iter()
            .map(|e| {
                let props: std::collections::BTreeMap<String, serde_json::Value> =
                    serde_json::from_str(e.extra_properties.as_deref().unwrap_or("{}")).unwrap();
                format!(
                    "{} {}",
                    e.screen_name,
                    serde_json::to_string(&props).unwrap()
                )
            })
            .collect()
    }

    // Mobile's `when`: the first load of the run, then by realm; a timeout fires once and the
    // showing still ends; a scene the player is let into regardless ends it as failed.
    #[test]
    fn start_end_and_timeout_as_mobile_reports_them() {
        use ReturnedToWorld::*;
        let mut screen = LoadingScreen::default();
        assert!(screen.tick(false, None, "main", "0,0", 0.1).is_empty());
        assert_eq!(
            names(&screen.tick(true, None, "main", "1,2", 0.1)),
            [r#"LOADING_START {"position":"1,2","realm":"main","when":"on_explorer_ready"}"#]
        );
        assert!(screen.tick(true, None, "main", "1,2", 1.0).is_empty());
        // the outcome lands the frame the player is released, the hiding follows
        assert!(screen
            .tick(true, Some(SceneReady), "main", "1,2", 0.1)
            .is_empty());
        assert_eq!(
            names(&screen.tick(false, None, "main", "1,2", 0.1)),
            [r#"LOADING_END {"position":"1,2","status":"Success"}"#]
        );

        assert_eq!(
            names(&screen.tick(true, None, "main", "50,60", 0.1)),
            [r#"LOADING_START {"position":"50,60","realm":"main","when":"on_moveto"}"#]
        );
        assert_eq!(
            names(&screen.tick(true, None, "main", "50,60", LOADING_TIMEOUT_SECS)),
            [r#"LOADING_TIMEOUT {"timeout_reason":"max_time","total_elapsed_seconds":90}"#]
        );
        assert!(screen.tick(true, None, "main", "50,60", 100.0).is_empty());
        assert_eq!(
            names(&screen.tick(false, Some(SceneBroken), "main", "50,60", 0.1)),
            [r#"LOADING_END {"error":"scene_broken","position":"50,60","status":"Failed"}"#]
        );

        assert_eq!(
            names(&screen.tick(true, None, "world.dcl.eth", "0,0", 0.1)),
            [r#"LOADING_START {"position":"0,0","realm":"world.dcl.eth","when":"on_changerealm"}"#]
        );
        assert_eq!(
            names(&screen.tick(false, Some(SceneLoadFailed), "world.dcl.eth", "0,0", 0.1)),
            [r#"LOADING_END {"error":"scene_load_failed","position":"0,0","status":"Failed"}"#]
        );
    }
}
