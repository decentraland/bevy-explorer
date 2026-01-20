// system api module

const { 
    op_check_for_update, op_motd, 
    op_get_current_login, op_get_previous_login, op_login_previous, op_login_new_code, op_login_new_success, op_login_cancel, op_login_guest, op_logout,
    op_settings, op_set_setting,
} = Deno.core.ops;

// (description: option<string>, url: option<string>)
module.exports.checkForUpdate = async function() {
    const [description, url] = await op_check_for_update();
    return {
        description,
        url
    }
}

// (message: string)
module.exports.messageOfTheDay = async function() {
    return {
        message: await op_motd()
    }
}

// (currentLogin: option<string>)
module.exports.getCurrentLogin = function() {
    return {
        current_login: op_get_current_login()
    }
}

// (userId: option<string>)
module.exports.getPreviousLogin = async function() {
    return {
        userId: await op_get_previous_login()
    }
}

// (success: bool, error: option<string>)
module.exports.loginPrevious = async function() {
    return await op_login_previous()
}

// (code: Promise<option<string>>, success: Promise<(success: bool, error: option<string>)>)
module.exports.loginNew = function() {
    return {
        code: op_login_new_code(),
        success: op_login_new_success(),
    }
}

// nothing
module.exports.loginGuest = function() {
    op_login_guest()
}

// nothing
module.exports.loginCancel = function() {
    op_login_cancel()
}

// nothing
module.exports.logout = function() {
    op_logout()
}

// array of {
//   name: string, 
//   category: string, 
//   description: string,
//   minValue: number, 
//   maxValue: number, 
//   namedVariants: [(variantName: string, variantDescription: string)]
//   value: number,
//   default: number,
// }
module.exports.getSettings = async function() {
    return await op_settings();
}

module.exports.setSetting = async function(name, value) {
    await op_set_setting(name, value);
}

module.exports.kernelFetch = async function (body) { 
    const headers = await Deno.core.ops.op_kernel_fetch_headers(body.url, body.init?.method, body.meta);

    if (!body.init) {
        body.init = { headers: {} };
    }

    if (!body.init.hasOwnProperty("headers")) {
        body.init.headers = {};
    }

    for (var i=0; i< headers.length; i++) {
        body.init.headers[headers[i][0]] = headers[i][1];
    }

    let response = await fetch(body.url, body.init);
    let text = await response.text();

    return {
        ok: response.ok,
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
        body: text,
    };
}

// avatar { 
//   base?: PBAvatarBase, 
//   equip?: PBAvatarEquippedData,
//   hasClaimedName?: bool,
//   profileExtras?: {field: value}
// }
// => deployed version
module.exports.setAvatar = async function(avatar) {
    return await Deno.core.ops.op_set_avatar(avatar.base, avatar.equip, avatar.hasClaimedName, avatar.profileExtras)
}

module.exports.getProfileExtras = async function() {
    return await Deno.core.ops.op_get_profile_extras();
}

// get the next key/button pressed by the user, identified as a string
// -> string
module.exports.getNativeInput = async function() {
    return await Deno.core.ops.op_native_input()
}

// get current key bindings
// -> {
//   bindings: (string, string[])[]
// }
module.exports.getInputBindings = async function() {
    return await Deno.core.ops.op_get_bindings()
}

// set current key bindings
// arg: {
//   bindings: (string, string[])[]
// }
module.exports.setInputBindings = async function(bindings) {
    await Deno.core.ops.op_set_bindings(bindings)
}


/// reload 
// hash: string | undefined
// if hash is provided, that specific scene is reloaded
// if unspecified, all scenes are reloaded
module.exports.reload = async function(hash) {
    await Deno.core.ops.op_console_command("reload", hash !== undefined ? [hash] : [])
}

// show_ui
// {
//   hash: string | undefined,
//   show: bool | undefined
// }
//
// if hash is undefined, all scenes are modified
// if show is undefined, acts as a toggle
// returns: visible bool 
// note: doesn't hide/show the system ui scene
module.exports.showUi = async function(args) {
  let argsArray = [args?.hash ?? "all"]
  if (args?.show !== undefined) {
    argsArray.push(args.show ? "true" : "false")
  }

  const reply = await Deno.core.ops.op_console_command("show_ui", argsArray)
  const value = reply.split(":").pop()?.trim().toLowerCase();
  return value === "true";  
}

// [{
//     hash: string,
//     base_url?: string,
//     title: string,
//     parcels: [Vector2],
//     isPortable: bool,
//     isBroken: bool,
//     isBlocked: bool,
//     isSuper: bool,
//     sdkVersion: string,
// }]
module.exports.liveSceneInfo = async function() {
    return await Deno.core.ops.op_live_scene_info()
}

// { 
//   realm: string, // about_url (e.g. `realm-provider.decentraland.org/main`)
//   parcel: Vector2,
// }
module.exports.getHomeScene = async function() {
    return await Deno.core.ops.op_get_home_scene()
}

// { 
//   realm: string, // about_url (e.g. `realm-provider.decentraland.org/main`)
//   parcel: Vector2,
// }
module.exports.setHomeScene = async function(args) {
    await Deno.core.ops.op_set_home_scene(args.realm, args.parcel)
}

// get system actions as a stream
// type SystemAction = {
//   action: string,
//   pressed: boolean,
// }
module.exports.getSystemActionStream = async function() {
  const rid = await Deno.core.ops.op_get_system_action_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_system_action_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}

// get chat messages as a stream
// type ChatMessage = {
//   senderAddress: string,
//   message: string,
//   channel: string,
// }
module.exports.getChatStream = async function() {
  const rid = await Deno.core.ops.op_get_chat_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_chat_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}

// send a chat message
// { 
//   message: string,
//   channel?: string
// }
module.exports.sendChat = async function(message, channel) {
    Deno.core.ops.op_send_chat(message, channel ?? "Nearby")
}

module.exports.quit = function() {
    Deno.core.ops.op_quit();
}

module.exports.getPermissionRequestStream = async function() {
  const rid = await Deno.core.ops.op_get_permission_request_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_permission_request_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}

module.exports.getPermissionUsedStream = async function() {
  const rid = await Deno.core.ops.op_get_permission_used_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_permission_used_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}

module.exports.setSinglePermission = function(body) {
    Deno.core.ops.op_set_single_permission(body.id, body.allow);
}

module.exports.setPermanentPermission = function(body) {
    Deno.core.ops.op_set_permanent_permission(body.level, body.value, body.ty, body.allow)
}

module.exports.getPermanentPermissions = async function(body) {
    return await Deno.core.ops.op_get_permanent_permissions(body.level, body.value);
}

module.exports.getPermissionTypes = function() {
    return Deno.core.ops.getPermissionTypes();
}

module.exports.setInteractableArea = function(body) {
    Deno.core.ops.op_set_interactable_area(body.left, body.top, body.right, body.bottom);
}

module.exports.getMicState = function() {
    return Deno.core.ops.op_get_mic_state();
}

module.exports.setMicEnabled = function(enabled) {
    Deno.core.ops.op_set_mic_enabled(enabled);
}

// get voice stream / mic activations as a stream
// type MicActivation = {
//   senderAddress: string,
//   active: bool,
// }
module.exports.getVoiceStream = async function() {
  const rid = await Deno.core.ops.op_get_voice_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_voice_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}

// get hover events as a stream
// HoverTargetType: 0 = World, 1 = Ui, 2 = Avatar
// PointerEventType: 0 = PET_UP, 1 = PET_DOWN, 2 = PET_HOVER_ENTER, 3 = PET_HOVER_LEAVE, 4 = PET_DRAG_LOCKED, 5 = PET_DRAG, 6 = PET_DRAG_END
// type HoverEventInfo = {
//   inputAction: number,   // InputAction (0-13)
//   hoverText: string,
//   hideFeedback: boolean,
//   showHighlight: boolean,
//   maxDistance: number,
// }
// type HoverAction = {
//   eventType: number,     // PointerEventType (0-6)
//   eventInfo: HoverEventInfo,
// }
// type HoverEvent = {
//   entered: boolean,
//   targetType: HoverTargetType,
//   distance: number,
//   actions: HoverAction[],
//   outsideScene: boolean,  // true if player is outside the scene containing the entity (always false for avatars)
// }
module.exports.getHoverStream = async function() {
  const rid = await Deno.core.ops.op_get_hover_stream();

  async function* streamGenerator() {
    while (true) {
      const next = await Deno.core.ops.op_read_hover_stream(rid);
      if (next === null) break;
      yield next;
    }
  }

  return streamGenerator();
}
