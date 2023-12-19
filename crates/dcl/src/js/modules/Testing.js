const testingEnabled = Deno.core.ops.op_testing_enabled();

function emptyTesting() {
    return {
        logTestResult: async function (body) { return {} },
        plan: async function (body) { return {} },
        setCameraTransform: async function (body) { return {} },
        takeAndCompareScreenshot: async function (body) { return {} }
    }
}

function testingModule() {
    function takeAndCompareScreenshot(body) {
        const { srcStoredSnapshot, cameraPosition, cameraTarget, screenshotSize } = body
        const methods = {
            grey_pixel_diff: body.greyPixelDiff
        }

        console.log({ methods })
        return Deno.core.ops.op_take_and_compare_snapshot(
            srcStoredSnapshot,
            [cameraPosition.x, cameraPosition.y, cameraPosition.z],
            [cameraTarget.x, cameraTarget.y, cameraTarget.z],
            [screenshotSize.x, screenshotSize.y],
            methods
        );
    }

    return {
        logTestResult: async function (body) {
            Deno.core.ops.op_log_test_result(body);
            return {}
        },
        plan: async function (body) {
            Deno.core.ops.op_log_test_plan(body);
            return {}
        },
        setCameraTransform: async function (body) { return {} },
        takeAndCompareScreenshot
    }
}

module.exports = testingEnabled ? testingModule() : emptyTesting()
