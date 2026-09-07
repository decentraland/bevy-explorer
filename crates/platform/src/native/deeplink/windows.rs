use std::path::PathBuf;

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
    // `    (Default)    REG_SZ    "C:\...\some.exe" "%1"`: ours if the exe is named like us
    let Some(path) = command.split('"').nth(1).map(PathBuf::from) else {
        return Handler::Other;
    };
    let ours = exe_path()
        .ok()
        .and_then(|exe| exe.file_name().map(|name| name.to_owned()))
        .is_some_and(|name| path.file_name() == Some(name.as_os_str()));
    if ours {
        Handler::Ours(path)
    } else {
        Handler::Other
    }
}

pub fn exe_path() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::current_exe()?)
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

    let exe = exe_path()?;
    let key = format!("HKCU\\Software\\Classes\\{SCHEME}");
    let command_key = format!("{key}\\shell\\open\\command");
    let command = format!("\"{}\" \"%1\"", exe.display());
    reg_add(&[&key, "/ve", "/d", "URL:Decentraland"])?;
    reg_add(&[&key, "/v", "URL Protocol", "/d", ""])?;
    reg_add(&[&command_key, "/ve", "/d", &command])?;
    bevy::log::info!("registered {} as the {SCHEME}:// handler", exe.display());
    Ok(())
}
