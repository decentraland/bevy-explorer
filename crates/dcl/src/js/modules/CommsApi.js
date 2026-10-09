// Shapes match the CommsApi protocol (and hammurabi's mocks). Topic pub/sub is not
// wired to the transport, but scenes must get the documented shapes back rather than
// undefined (which throws on the caller's `.streams`/`.messages` access).
module.exports.getActiveVideoStreams = async function (body) {
    const streams = await Deno.core.ops.op_get_active_video_streams()
    return { streams }
}

module.exports.subscribeToTopic = async function (body) {
    return {}
}

module.exports.unsubscribeFromTopic = async function (body) {
    return {}
}

module.exports.publishData = async function (body) {
    return {}
}

module.exports.consumeMessages = async function (body) {
    return { messages: [] }
}
