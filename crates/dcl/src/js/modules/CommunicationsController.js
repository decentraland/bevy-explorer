module.exports.send = async function (body) {
    await Deno.core.ops.op_comms_send_string(body.message, "");
    return {}
}

module.exports.sendBinary = async function (body) {
    // old style
    for (const buffer of body.data) {
        await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer));
    }
    // new style
    for (const peerData of body.peerData) {
        if (Array.isArray(peerData.address) && peerData.address.length > 0) {
            for (const address of peerData.address) {
                for (const buffer of peerData.data) {
                    await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer), address);
                }
            }
        } else {
            for (const buffer of peerData.data) {
                await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer), null);
            }
        }
    }

    const data = (await Deno.core.ops.op_comms_recv_binary()).map((item) => new Uint8Array(item));
    return {
        data
    }
}
