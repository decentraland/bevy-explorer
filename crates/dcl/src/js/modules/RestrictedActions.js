module.exports.movePlayerTo = async function (body) { 
    if ("cameraTarget" in body) {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z], [body.cameraTarget.x, body.cameraTarget.y, body.cameraTarget.z]);
    } else {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z]);
    }
    return {} 
}

module.exports.teleportTo = async function (body) { 
    Deno.core.ops.op_move_player_to(true, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z])
    return {} 
}

module.exports.triggerEmote = async function (body) { return {} }

module.exports.changeRealm = async function (body) { 
    return await Deno.core.ops.op_change_realm(body.realm, body.message);
}

module.exports.openExternalUrl = async function (body) { return {} }
module.exports.openNftDialog = async function (body) { return {} }
module.exports.setCommunicationsAdapter = async function (body) { return {} }