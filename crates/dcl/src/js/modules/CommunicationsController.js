// Cap one sendBinary call so a scene can't amplify it into a broadcast storm.
const MAX_SEND_PEERS = 256;
const MAX_SEND_MESSAGES = 512;
const MAX_COMMS_MESSAGE_BYTES = 30000;

module.exports.send = async function (body) {
    await Deno.core.ops.op_comms_send_string(body.message, "");
    return {}
}

module.exports.sendBinary = async function (body) {
    let messageCount = 0;
    const peers = new Set();
    // old style
    for (const buffer of body.data) {
        if (messageCount >= MAX_SEND_MESSAGES) break;
        if (buffer.byteLength > MAX_COMMS_MESSAGE_BYTES) continue;
        await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer));
        messageCount++;
    }
    // new style
    if (body.peerData !== undefined) {
        for (const peerData of body.peerData) {
            if (messageCount >= MAX_SEND_MESSAGES) break;
            if (Array.isArray(peerData.address) && peerData.address.length > 0) {
                for (const address of peerData.address) {
                    if (!peers.has(address) && peers.size >= MAX_SEND_PEERS) continue;
                    peers.add(address);
                    for (const buffer of peerData.data) {
                        if (messageCount >= MAX_SEND_MESSAGES) break;
                        if (buffer.byteLength > MAX_COMMS_MESSAGE_BYTES) continue;
                        await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer), address);
                        messageCount++;
                    }
                }
            } else {
                for (const buffer of peerData.data) {
                    if (messageCount >= MAX_SEND_MESSAGES) break;
                    if (buffer.byteLength > MAX_COMMS_MESSAGE_BYTES) continue;
                    await Deno.core.ops.op_comms_send_binary_single(new Uint8Array(buffer), null);
                    messageCount++;
                }
            }
        }
    }

    const data = (await Deno.core.ops.op_comms_recv_binary()).map((item) => new Uint8Array(item));
    return {
        data
    }
}
