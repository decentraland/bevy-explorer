use dcl::js::testing::{
    SceneTestPlan, SceneTestResult, TakeAndCompareSnapshotResponse,
    TestingScreenshotComparisonMethodRequest,
};
use deno_core::{error::AnyError, op2, OpDecl, OpState};

#[derive(Default)]
pub struct TestPlan {
    pub tests: Vec<String>,
}

// list of op declarations
pub fn ops() -> Vec<OpDecl> {
    vec![
        op_testing_enabled(),
        op_take_and_compare_snapshot(),
        op_log_test_result(),
        op_log_test_plan(),
    ]
}

#[op2(fast)]
fn op_testing_enabled(op_state: &mut OpState) -> bool {
    dcl::js::testing::op_testing_enabled(op_state)
}

#[op2]
fn op_log_test_plan(state: &mut OpState, #[serde] body: SceneTestPlan) {
    dcl::js::testing::op_log_test_plan(state, body);
}

#[op2]
fn op_log_test_result(state: &mut OpState, #[serde] body: SceneTestResult) {
    dcl::js::testing::op_log_test_result(state, body);
}

#[op2]
#[serde]
fn op_take_and_compare_snapshot(
    state: &mut OpState,
    #[string] name: String,
    #[serde] camera_position: (f32, f32, f32),
    #[serde] camera_target: (f32, f32, f32),
    #[serde] snapshot_size: (u32, u32),
    #[serde] method: TestingScreenshotComparisonMethodRequest,
) -> Result<TakeAndCompareSnapshotResponse, AnyError> {
    dcl::js::testing::op_take_and_compare_snapshot(
        state,
        name,
        camera_position,
        camera_target,
        snapshot_size,
        method,
    )
}
