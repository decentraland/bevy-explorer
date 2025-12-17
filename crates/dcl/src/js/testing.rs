use std::time::Duration;

use bevy::log::debug;
use common::rpc::{CompareSnapshot, CompareSnapshotResult, RpcCall, RpcResultSender};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::error::TryRecvError;

use crate::{
    interface::crdt_context::CrdtContext, js::SceneResponseSender, RpcCalls, SceneResponse,
};

use super::State;

#[derive(Default)]
pub struct TestPlan {
    pub tests: Vec<String>,
}

pub fn op_testing_enabled(op_state: &mut impl State) -> bool {
    debug!("op_testing_enabled");
    op_state.borrow::<CrdtContext>().testing
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTestPlan {
    pub tests: Vec<SceneTestPlanTestPlanEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTestPlanTestPlanEntry {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTestResult {
    pub name: String,
    pub ok: bool,
    pub error: Option<String>,
    pub stack: Option<String>,
    pub total_frames: i32,
    pub total_time: f32,
}

pub fn op_log_test_plan(state: &mut impl State, body: SceneTestPlan) {
    debug!("op_log_test_plan");
    let scene = state.borrow::<CrdtContext>().scene_id.0;

    state.borrow_mut::<RpcCalls>().push(RpcCall::TestPlan {
        scene,
        plan: body.tests.into_iter().map(|p| p.name).collect(),
    });
}

pub fn op_log_test_result(state: &mut impl State, body: SceneTestResult) {
    debug!("op_log_test_results");
    let scene = state.borrow::<CrdtContext>().scene_id.0;

    state.borrow_mut::<RpcCalls>().push(RpcCall::TestResult {
        scene,
        name: body.name,
        success: body.ok,
        error: body.error,
    });
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GreyPixelDiffResult {
    pub similarity: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GreyPixelDiffRequest;

#[derive(Debug, Deserialize, Serialize)]
pub struct TestingScreenshotComparisonMethodRequest {
    grey_pixel_diff: Option<GreyPixelDiffRequest>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TakeAndCompareSnapshotResponse {
    pub stored_snapshot_found: bool,
    pub grey_pixel_diff: Option<GreyPixelDiffResult>,
}

pub fn op_take_and_compare_snapshot(
    state: &mut impl State,
    name: String,
    camera_position: (f32, f32, f32),
    camera_target: (f32, f32, f32),
    snapshot_size: (u32, u32),
    method: TestingScreenshotComparisonMethodRequest,
) -> Result<TakeAndCompareSnapshotResponse, anyhow::Error> {
    debug!("op_take_and_compare_snapshot");
    let camera_position = [camera_position.0, camera_position.1, camera_position.2];
    let camera_target = [camera_target.0, camera_target.1, camera_target.2];
    let snapshot_size = [snapshot_size.0, snapshot_size.1];

    let scene = state.borrow::<CrdtContext>().scene_id.0;
    let sender = state.borrow_mut::<SceneResponseSender>();

    if method.grey_pixel_diff.is_none() {
        anyhow::bail!("unsupported comparison format");
    }

    let (sx, mut rx) = RpcResultSender::channel();

    sender
        .try_send(SceneResponse::CompareSnapshot(CompareSnapshot {
            scene,
            camera_position,
            camera_target,
            snapshot_size,
            name,
            response: sx,
        }))
        .expect("failed to send to renderer");

    let (error, stored_snapshot_found, similarity) = loop {
        match rx.try_recv() {
            Ok(CompareSnapshotResult {
                error,
                found,
                similarity,
            }) => break (error, found, similarity),
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(100)),
            Err(TryRecvError::Closed) => anyhow::bail!("snapshot failed"),
        }
    };

    if let Some(err) = error {
        anyhow::bail!(err)
    } else {
        Ok(TakeAndCompareSnapshotResponse {
            stored_snapshot_found,
            grey_pixel_diff: Some(GreyPixelDiffResult { similarity }),
        })
    }
}
