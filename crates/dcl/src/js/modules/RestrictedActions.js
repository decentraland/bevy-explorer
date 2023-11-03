module.exports.movePlayerTo = async function (body) { 
    if ("cameraTarget" in body) {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z], [body.cameraTarget.x, body.cameraTarget.y, body.cameraTarget.z]);
    } else {
        Deno.core.ops.op_move_player_to(false, [body.newRelativePosition.x, body.newRelativePosition.y, body.newRelativePosition.z]);
    }
    return {} 
}

module.exports.teleportTo = async function (body) { 
    await Deno.core.ops.op_teleport_to([body.worldCoordinates.x, body.worldCoordinates.y]);
    return {} 
}

module.exports.triggerEmote = async function (body) { 
    // if only there was a way to run an ecs system here
    Deno.core.ops.op_emote(body.predefinedEmote)
    return {} 
}

module.exports.changeRealm = async function (body) { 
    return await Deno.core.ops.op_change_realm(body.realm, body.message);
}

module.exports.openExternalUrl = async function (body) { 
    return await Deno.core.ops.op_external_url(body.url);
}

module.exports.openNftDialog = async function (body) { return {} }
module.exports.setCommunicationsAdapter = async function (body) { 
    console.error("RestrictedActions::setCommunicationsAdapter not implemented");
    return {} 
}
