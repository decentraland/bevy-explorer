use std::path::{Path, PathBuf};

use winreg::{
    enums::{HKEY_CLASSES_ROOT, HKEY_CURRENT_USER},
    RegKey,
};

use super::{Handler, SCHEME};

pub fn handler() -> Handler {
    // HKCR merges the machine and user registrations
    let Ok(key) =
        RegKey::predef(HKEY_CLASSES_ROOT).open_subkey(format!("{SCHEME}\\shell\\open\\command"))
    else {
        return Handler::None;
    };
    let Ok(command) = key.get_value::<String, _>("") else {
        return Handler::Other;
    };
    // `"C:\...\some.exe" "%1"`; an unquoted command is someone else's and left alone
    let Some(exe) = command
        .trim()
        .strip_prefix('"')
        .and_then(|rest| rest.split('"').next())
        .map(Path::new)
    else {
        return Handler::Other;
    };
    // a registration whose exe is gone (an uninstalled launcher) is nobody's
    if !exe.exists() {
        return Handler::None;
    }
    let ours = current_exe()
        .ok()
        .and_then(|ours| ours.file_name().map(|name| name.to_owned()))
        .is_some_and(|name| exe.file_name() == Some(name.as_os_str()));
    if ours {
        Handler::Ours(command)
    } else {
        Handler::Other
    }
}

/// The `shell\open\command` for this binary.
pub fn registration() -> Result<String, anyhow::Error> {
    Ok(format!("\"{}\" \"%1\"", current_exe()?.display()))
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    let command = registration()?;
    let write = || -> std::io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(format!("Software\\Classes\\{SCHEME}"))?;
        key.set_value("", &"URL:Decentraland")?;
        key.set_value("URL Protocol", &"")?;
        let (command_key, _) = key.create_subkey("shell\\open\\command")?;
        command_key.set_value("", &command)
    };
    write().map_err(|e| anyhow::anyhow!("failed to register the {SCHEME}:// handler: {e}"))?;
    bevy::log::info!("registered {command} as the {SCHEME}:// handler");
    Ok(())
}

fn current_exe() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::current_exe()?)
}
