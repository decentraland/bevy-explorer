//! `openExplorerUi` restricted action + the `ExplorerUiEventsResult` lifecycle events.
//!
//! The panels are the react HUD's fullscreen menu pages (`HudPanel`), each toggled by a
//! `SystemAction`. An accepted request synthesizes that action's press/release edge on the
//! system-action stream, so the HUD opens the page exactly as it would for the bound key. The HUD
//! reports the page it shows back as `SystemApi::SetUiFocus { menu }`; that report both
//! answers `WAS_ALREADY_OPEN` and drives the `opened` / `closed` events, which (as in unity) go
//! only to the scene whose request opened the page.

use bevy::prelude::*;
use common::{
    inputs::HudPanel,
    rpc::{OpenExplorerUiResult, RpcCall, RpcResultSender},
    structs::{PermissionType, PrimaryUser},
};
use dcl_component::{
    proto_components::sdk::components::{
        common::ExplorerUi,
        pb_explorer_ui_events_result::{Event as UiEvent, UiClosed, UiOpened},
        PbExplorerUiEventsResult,
    },
    CrdtType, SceneComponentId, SceneEntityId,
};
use input_manager::SystemActionStreams;
use scene_runner::{
    permissions::Permission, renderer_context::RendererSceneContext, ContainingScene,
};
use system_bridge::SystemApi;

/// How long an accepted request waits for the HUD to report its page open before it is answered
/// as not opened. The HUD drops action edges while a popup is up or a text field is focused, and
/// never reports for a dropped edge.
const PENDING_TIMEOUT_SECS: f32 = 2.0;

/// The HUD menu page an `ExplorerUi` panel maps onto. None: no such page in this client.
fn panel(ui: ExplorerUi) -> Option<HudPanel> {
    match ui {
        ExplorerUi::EuSettings => Some(HudPanel::Settings),
        ExplorerUi::EuMap => Some(HudPanel::Map),
        ExplorerUi::EuBackpack => Some(HudPanel::Backpack),
        ExplorerUi::EuCameraReel => Some(HudPanel::Gallery),
        ExplorerUi::EuCommunities => Some(HudPanel::Communities),
        ExplorerUi::EuPlaces => Some(HudPanel::Places),
        ExplorerUi::EuEvents => None,
    }
}

pub struct SceneOpen {
    scene: Entity,
    /// raw `ExplorerUi` value, echoed into the events
    ui: i32,
    panel: HudPanel,
}

/// An accepted request whose edge has been sent: unanswered until the HUD reports the page.
pub struct PendingOpen {
    open: SceneOpen,
    response: RpcResultSender<OpenExplorerUiResult>,
    deadline: f32,
}

#[derive(Resource, Default)]
pub struct ExplorerUiState {
    /// the menu page the HUD last reported open
    open: Option<HudPanel>,
    pending: Option<PendingOpen>,
    /// the scene-opened page currently showing; its `closed` event goes to that scene
    tracked: Option<SceneOpen>,
}

pub type OpenRequest = (SceneOpen, RpcResultSender<OpenExplorerUiResult>);

#[allow(clippy::too_many_arguments)]
pub fn open_explorer_ui(
    mut events: EventReader<RpcCall>,
    mut perms: Permission<OpenRequest>,
    containing_scene: ContainingScene,
    player: Query<Entity, With<PrimaryUser>>,
    scenes: Query<&RendererSceneContext>,
    streams: Res<SystemActionStreams>,
    mut state: ResMut<ExplorerUiState>,
    time: Res<Time>,
) {
    for (scene, ui, response) in events.read().filter_map(|ev| match ev {
        RpcCall::OpenExplorerUi {
            scene,
            ui,
            response,
        } => Some((*scene, *ui, response.clone())),
        _ => None,
    }) {
        let in_scene = player
            .single()
            .is_ok_and(|p| containing_scene.get(p).contains(&scene));
        if !in_scene {
            response.send(OpenExplorerUiResult::RejectedNotCurrentScene);
            continue;
        }
        // same recent-pointer-action window as the eth / clipboard actions
        let last_action_time = scenes
            .get(scene)
            .ok()
            .and_then(|scene| scene.last_action_event)
            .unwrap_or_default();
        if last_action_time < time.elapsed_secs_f64() - 1.0 {
            response.send(OpenExplorerUiResult::RejectedNoUserGesture);
            continue;
        }
        let Some(panel) = ExplorerUi::from_i32(ui).and_then(panel) else {
            response.send(OpenExplorerUiResult::RejectedFeatureDisabled);
            continue;
        };
        // nothing consumes the action stream (native ui, headless): no page can open
        if streams.is_empty() {
            response.send(OpenExplorerUiResult::RejectedFeatureDisabled);
            continue;
        }
        perms.check(
            PermissionType::OpenExplorerUi,
            scene,
            (SceneOpen { scene, ui, panel }, response),
            None,
            // the current-scene gate above already applied
            true,
        );
    }

    for (open, response) in perms.drain_success(PermissionType::OpenExplorerUi) {
        // a page is showing, or another request's edge is in flight (a second edge would
        // toggle that page straight back off)
        if state.open.is_some() || state.pending.is_some() {
            response.send(OpenExplorerUiResult::WasAlreadyOpen);
            continue;
        }
        // answered by track_explorer_ui once the HUD reports the page (or not)
        streams.send_edge(open.panel.action());
        state.pending = Some(PendingOpen {
            open,
            response,
            deadline: time.elapsed_secs() + PENDING_TIMEOUT_SECS,
        });
    }

    for (_, response) in perms.drain_fail(PermissionType::OpenExplorerUi) {
        response.send(OpenExplorerUiResult::RejectedFeatureDisabled);
    }
}

pub fn track_explorer_ui(
    mut events: EventReader<SystemApi>,
    mut state: ResMut<ExplorerUiState>,
    mut scenes: Query<&mut RendererSceneContext>,
    time: Res<Time>,
) {
    for open in events.read().filter_map(|ev| match ev {
        SystemApi::SetUiFocus { menu, .. } => Some(*menu),
        _ => None,
    }) {
        if open == state.open {
            continue;
        }
        // the scene-opened page went away (closed, or replaced by another page)
        if state
            .tracked
            .as_ref()
            .is_some_and(|t| open != Some(t.panel))
        {
            let closed = state.tracked.take().unwrap();
            write_event(&mut scenes, &closed, UiEvent::Closed(UiClosed {}));
        }
        // a pending request is OPENED only if the HUD shows the page it asked for; any other
        // page means the edge was superseded (the user opened something first, and that page is
        // what is open now); nothing showing means the edge was swallowed, keep waiting
        if let Some(pending) = state.pending.take() {
            if open == Some(pending.open.panel) {
                write_event(&mut scenes, &pending.open, UiEvent::Opened(UiOpened {}));
                pending.response.send(OpenExplorerUiResult::Opened);
                state.tracked = Some(pending.open);
            } else if open.is_some() {
                pending.response.send(OpenExplorerUiResult::WasAlreadyOpen);
            } else {
                state.pending = Some(pending);
            }
        }
        state.open = open;
    }

    // a dropped edge never produces a report: the HUD had a popup up or a text field focused, so
    // some HUD surface was in the way — the nearest verdict the protocol offers
    if state
        .pending
        .as_ref()
        .is_some_and(|p| time.elapsed_secs() > p.deadline)
    {
        let pending = state.pending.take().unwrap();
        pending.response.send(OpenExplorerUiResult::WasAlreadyOpen);
    }
}

fn write_event(scenes: &mut Query<&mut RendererSceneContext>, open: &SceneOpen, event: UiEvent) {
    // the scene may have unloaded while its page was showing
    let Ok(mut context) = scenes.get_mut(open.scene) else {
        return;
    };
    let timestamp = context.tick_number;
    context.update_crdt(
        SceneComponentId::EXPLORER_UI_EVENTS_RESULT,
        CrdtType::GO_ENT,
        SceneEntityId::ROOT,
        &PbExplorerUiEventsResult {
            ui: open.ui,
            timestamp,
            event: Some(event),
        },
    );
}
