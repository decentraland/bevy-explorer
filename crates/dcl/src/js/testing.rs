use std::{sync::mpsc::SyncSender, time::Duration};

use common::rpc::{CompareSnapshot, CompareSnapshotResult, RpcCall};
use deno_core::{anyhow, error::AnyError, op, Op, OpDecl, OpState};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::{channel, error::TryRecvError};

use crate::{interface::crdt_context::CrdtContext, RpcCalls, SceneResponse};

#[derive(Default)]
pub struct TestPlan {
    pub tests: Vec<String>,
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_testing_enabled::DECL,
        op_take_and_compare_snapshot::DECL,
        op_log_test_result::DECL,
        op_log_test_plan::DECL,
    ]
}

#[op]
fn op_testing_enabled(op_state: &mut OpState) -> bool {
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

#[op]
fn op_log_test_plan(state: &mut OpState, body: SceneTestPlan) {
    let scene = state.borrow::<CrdtContext>().scene_id.0;

    state.borrow_mut::<RpcCalls>().push(RpcCall::TestPlan {
        scene,
        plan: body.tests.into_iter().map(|p| p.name).collect(),
    });
}

#[op]
fn op_log_test_result(state: &mut OpState, body: SceneTestResult) {
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

#[op]
fn op_take_and_compare_snapshot(
    state: &mut OpState,
    name: String,
    camera_position: [f32; 3],
    camera_target: [f32; 3],
    snapshot_size: [u32; 2],
    method: TestingScreenshotComparisonMethodRequest,
) -> Result<TakeAndCompareSnapshotResponse, AnyError> {
    let scene = state.borrow::<CrdtContext>().scene_id.0;
    let sender = state.borrow_mut::<SyncSender<SceneResponse>>();

    if method.grey_pixel_diff.is_none() {
        anyhow::bail!("unsupported comparison format");
    }

    let (sx, mut rx) = channel();

    sender
        .send(SceneResponse::CompareSnapshot(CompareSnapshot {
            scene,
            camera_position,
            camera_target,
            snapshot_size,
            name,
            response: sx.into(),
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
