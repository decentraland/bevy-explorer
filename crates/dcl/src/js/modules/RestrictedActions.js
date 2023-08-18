module.exports.movePlayerTo = async function (body) { 
    const target = body.cameraTarget || {x: 0, y: 0, z: 0};
    Deno.core.ops.op_move_player_to([body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z], [target.x, target.y, target.z])
    return {} 
}

module.exports.teleportTo = async function (body) { return {} }
module.exports.triggerEmote = async function (body) { return {} }
module.exports.changeRealm = async function (body) { return {} }
module.exports.openExternalUrl = async function (body) { return {} }
module.exports.openNftDialog = async function (body) { return {} }
module.exports.setCommunicationsAdapter = async function (body) { return {} }