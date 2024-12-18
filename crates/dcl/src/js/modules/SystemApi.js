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
