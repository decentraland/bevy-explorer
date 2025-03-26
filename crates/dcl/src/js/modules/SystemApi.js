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
//   base: PBAvatarBase, 
//   equip: PBAvatarEquippedData,
// }
// => deployed version
module.exports.setAvatar = async function(avatar) {
    return await Deno.core.ops.op_set_avatar(avatar.base, avatar.equip)
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
