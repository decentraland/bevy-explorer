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
        .map(expand_env)
    else {
        return Handler::Other;
    };
    let exe = Path::new(&exe);
    // a registration whose exe is gone (an uninstalled launcher) is nobody's
    if !exe.exists() {
        return Handler::None;
    }
    // the filesystem is case-insensitive
    let ours = current_exe()
        .ok()
        .is_some_and(|ours| match (ours.file_name(), exe.file_name()) {
            (Some(ours), Some(exe)) => ours
                .to_string_lossy()
                .eq_ignore_ascii_case(&exe.to_string_lossy()),
            _ => false,
        });
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

/// `%VAR%` references expanded the way the shell does before it launches the handler. A
/// `REG_EXPAND_SZ` registration stores them as written and winreg hands them back that way, so
/// without this a perfectly live `%LOCALAPPDATA%\...` exe looks like one that is gone.
fn expand_env(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some((before, after)) = rest.split_once('%') {
        // an unterminated `%` is literal, and so is the whole tail after it
        let Some((name, tail)) = after.split_once('%') else {
            break;
        };
        expanded.push_str(before);
        match std::env::var(name) {
            Ok(value) => expanded.push_str(&value),
            // `%%` and an unset variable are left as written, as the shell leaves them
            Err(_) => {
                expanded.push('%');
                expanded.push_str(name);
                expanded.push('%');
            }
        }
        rest = tail;
    }
    expanded.push_str(rest);
    expanded
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn env_references_expand() {
        let system_root = std::env::var("SystemRoot").unwrap();
        assert_eq!(
            expand_env("%SystemRoot%\\a.exe"),
            format!("{system_root}\\a.exe")
        );
        // left as written: an unset variable, an unterminated `%`, a literal `%%`
        assert_eq!(expand_env("%DclNotSet%\\a.exe"), "%DclNotSet%\\a.exe");
        assert_eq!(expand_env("C:\\a %b.exe"), "C:\\a %b.exe");
        assert_eq!(expand_env("C:\\100%%\\a.exe"), "C:\\100%%\\a.exe");
        assert_eq!(expand_env("C:\\plain\\a.exe"), "C:\\plain\\a.exe");
    }
}
