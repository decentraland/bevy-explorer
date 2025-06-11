module.exports.movePlayerTo = async function (body) {
    Deno.core.ops.op_move_player_to(
        body.newRelativePosition.x, 
        body.newRelativePosition.y, 
        body.newRelativePosition.z, 
        body.cameraTarget !== undefined, 
        body.cameraTarget?.x ?? 0, 
        body.cameraTarget?.y ?? 0, 
        body.cameraTarget?.z ?? 0,
        body.avatarTarget !== undefined,
        body.avatarTarget?.x ?? 0, 
        body.avatarTarget?.y ?? 0, 
        body.avatarTarget?.z ?? 0,
    );
    return {} 
}

module.exports.teleportTo = async function (body) { 
    await Deno.core.ops.op_teleport_to(Number(body.worldCoordinates.x), Number(body.worldCoordinates.y));
    return {} 
}

module.exports.triggerEmote = async function (body) { 
    Deno.core.ops.op_emote(body.predefinedEmote)
    return {} 
}

module.exports.triggerSceneEmote = async function (body) { 
    Deno.core.ops.op_scene_emote(body.src, body.looping)
    return {} 
}

module.exports.changeRealm = async function (body) { 
    return await Deno.core.ops.op_change_realm(body.realm, body.message);
}

module.exports.openExternalUrl = async function (body) { 
    return await Deno.core.ops.op_external_url(body.url);
}

module.exports.openNftDialog = async function (body) { 
    return await Deno.core.ops.op_open_nft_dialog(body.urn) 
}
module.exports.setCommunicationsAdapter = async function (body) { 
    console.error("RestrictedActions::setCommunicationsAdapter not implemented");
    return {} 
}
module.exports.setUiFocus = async function(body) {
    return await Deno.core.ops.op_set_ui_focus(body.elementId);
}

module.exports.copyToClipboard = async function(body) {
    return await Deno.core.ops.op_copy_to_clipboard(body.text);
}
