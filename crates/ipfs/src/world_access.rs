//! Entry to worlds. A world (a `/world/<name>` realm on the configured worlds server) can
//! restrict who gets in; the worlds server enforces that at the comms handshake, so an unchecked
//! realm change lands the player in the world's scenes but outside its rooms. A realm change to
//! a world is therefore held until the [`RealmGate`] says the player may enter. The check itself
//! lives with the identity it needs (`comms::world_access`); this crate only knows the shape.

use bevy::prelude::*;
use common::base_domain::{self, Service};
use tokio::sync::{mpsc, oneshot, watch};

pub const SUPERSEDED: &str = "superseded by a later realm change";

/// Whether a later realm change has been issued since the one this belongs to, which then stands
/// down rather than finish behind it.
#[derive(Clone)]
pub struct Superseded {
    pub latest: watch::Receiver<u64>,
    pub id: u64,
}

impl Superseded {
    pub fn is_superseded(&self) -> bool {
        *self.latest.borrow() != self.id
    }

    /// Resolves once superseded (or once nothing issues changes any more)
    pub async fn wait(&mut self) {
        let _ = self.latest.wait_for(|latest| *latest != self.id).await;
    }
}

/// The world's name when `realm` (a realm url, see `map_realm_name`) is a world on the configured
/// worlds server.
pub fn world_name(realm: &str) -> Option<String> {
    let worlds = base_domain::service(Service::WorldsServer);
    let name = realm
        .strip_prefix(worlds.as_str())?
        .strip_prefix("/world/")?
        .split(['/', '?', '#'])
        .next()
        .filter(|name| !name.is_empty())?;
    Some(name.to_owned())
}

/// A password the HUD must ask for; answered with the password, or `None` when the player gives
/// up. Dropped unanswered when the realm change no longer needs it.
pub struct WorldPasswordPrompt {
    pub world: String,
    /// Why the previous answer was refused; `None` on the first ask
    pub error: Option<String>,
    pub answer: oneshot::Sender<Option<String>>,
}

/// Where password prompts go, installed by whatever shows them (the system bridge). Without it a
/// password-protected world can't be entered.
#[derive(Resource, Clone)]
pub struct WorldPasswordPrompter(pub mpsc::UnboundedSender<WorldPasswordPrompt>);

/// The validated password of the world being entered, for its signed comms login; `None` on
/// any other realm.
#[derive(Resource, Default)]
pub struct WorldCommsSecret(pub Option<String>);

/// A world the player asks to enter, for whoever installed the [`RealmGate`] to decide: answered
/// with the password it may be entered with, or the reason it may not, in words for the player.
pub struct RealmGateRequest {
    pub realm: String,
    pub world: String,
    /// A password that got them into this world before
    pub known_secret: Option<String>,
    pub superseded: Superseded,
    pub answer: oneshot::Sender<Result<Option<String>, String>>,
}

/// Where world entry checks go, installed by whatever has an identity to check with (comms).
/// Without it, worlds are entered unchecked.
#[derive(Resource, Clone)]
pub struct RealmGate(pub mpsc::UnboundedSender<RealmGateRequest>);

#[cfg(test)]
mod tests {
    use super::world_name;

    #[test]
    fn names_a_world_on_the_worlds_server() {
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/boedo.dcl.eth"),
            Some("boedo.dcl.eth".to_owned())
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/boedo.dcl.eth/"),
            Some("boedo.dcl.eth".to_owned())
        );
    }

    #[test]
    fn other_realms_are_not_worlds() {
        assert_eq!(
            world_name("https://realm-provider-ea.decentraland.org/main"),
            None
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.org/world/"),
            None
        );
        assert_eq!(
            world_name("https://worlds-content-server.decentraland.zone/world/boedo.dcl.eth"),
            None
        );
    }
}
