module.exports.requestTeleport = async function (body) {
    const { destination } = body;
    if (destination === 'magic' || destination === 'crowd') {
        return await Deno.core.ops.op_teleport_to([
            0,
            0,
        ]);
    } else if (!/^\-?\d+\,\-?\d+$/.test(destination)) {
        return await Promise.reject(`teleportTo: invalid destination ${destination}`)
    }

    const coords = destination.split(',');

    // Convert the separate parts to whole numbers.
    let x = parseInt(coords[0], 10);
    let y = parseInt(coords[1], 10);

    return await Deno.core.ops.op_teleport_to([
        x,
        y,
    ]);
}
