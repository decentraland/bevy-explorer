module.exports.movePlayerTo = async function (body) {
    const success = await Deno.core.ops.op_move_player_to(
        body.newRelativePosition,
        body.cameraTarget ?? null,
        body.avatarTarget ?? null,
        body.duration ?? null,
    );
    return { success }
}

module.exports.walkPlayerTo = async function (body) {
    const success = await Deno.core.ops.op_walk_player_to(
        body.newRelativePosition,
        body.stopThreshold,
        body.timeout ?? null,
    );
    return { success }
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
    Deno.core.ops.op_scene_emote(body.src, body.loop)
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
    return await Deno.core.ops.op_ui_focus(true, body.elementId);
}
module.exports.clearUiFocus = async function(body) {
    return await Deno.core.ops.op_ui_focus(true);
}
module.exports.getUiFocus = async function() {
    return await Deno.core.ops.op_ui_focus(false);
}
module.exports.copyToClipboard = async function(body) {
    return await Deno.core.ops.op_copy_to_clipboard(body.text);
}
