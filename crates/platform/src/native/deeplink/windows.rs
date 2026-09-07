use std::path::{Path, PathBuf};

use super::{Handler, SCHEME};

pub fn handler() -> Handler {
    // HKCR merges the machine and user registrations
    let Some(command) = std::process::Command::new("reg")
        .args([
            "query",
            &format!("HKCR\\{SCHEME}\\shell\\open\\command"),
            "/ve",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
    else {
        return Handler::None;
    };
    // `    (Default)    REG_SZ    "C:\...\some.exe" "%1"` (the value name is localised)
    let Some(command) = command
        .lines()
        .find_map(|line| line.split_once("REG_SZ"))
        .map(|(_, command)| command.trim())
    else {
        return Handler::Other;
    };
    let exe = command.split('"').nth(1).map(Path::new);
    // a registration whose exe is gone (an uninstalled launcher) is nobody's
    if exe.is_some_and(|exe| !exe.exists()) {
        return Handler::None;
    }
    let ours = current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|name| name.to_owned()))
        .is_some_and(|name| exe.and_then(Path::file_name) == Some(name.as_os_str()));
    if ours {
        Handler::Ours(command.to_owned())
    } else {
        Handler::Other
    }
}

/// The `shell\open\command` for this binary.
pub fn registration() -> Result<String, anyhow::Error> {
    Ok(format!("\"{}\" \"%1\"", current_exe()?.display()))
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    fn reg_add(args: &[&str]) -> Result<(), anyhow::Error> {
        let output = std::process::Command::new("reg")
            .arg("add")
            .args(args)
            .arg("/f")
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "failed to register the {SCHEME}:// handler: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    let command = registration()?;
    let key = format!("HKCU\\Software\\Classes\\{SCHEME}");
    let command_key = format!("{key}\\shell\\open\\command");
    reg_add(&[&key, "/ve", "/d", "URL:Decentraland"])?;
    reg_add(&[&key, "/v", "URL Protocol", "/d", ""])?;
    reg_add(&[&command_key, "/ve", "/d", &command])?;
    bevy::log::info!("registered {command} as the {SCHEME}:// handler");
    Ok(())
}

fn current_exe() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::current_exe()?)
}
