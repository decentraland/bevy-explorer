use std::path::PathBuf;

use super::{Handler, SCHEME};

const DESKTOP_FILE: &str = "decentraland-bevy-deeplink.desktop";

pub fn handler() -> Handler {
    let Some(default) = std::process::Command::new("xdg-mime")
        .args(["query", "default", &format!("x-scheme-handler/{SCHEME}")])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|default| !default.is_empty())
    else {
        return Handler::None;
    };
    if default != DESKTOP_FILE {
        return Handler::Other;
    }
    applications_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(DESKTOP_FILE)).ok())
        .map_or(Handler::None, Handler::Ours)
}

/// The desktop file for this binary. Inside an AppImage `current_exe()` is the transient FUSE
/// mount, and the runtime exports the real path. A dev build finds libcef through
/// LD_LIBRARY_PATH, which the browser's environment lacks, so it is baked in.
pub fn registration() -> Result<String, anyhow::Error> {
    let exe = match std::env::var_os("APPIMAGE") {
        Some(path) => PathBuf::from(path),
        None => std::env::current_exe()?,
    };
    let env = std::env::var("LD_LIBRARY_PATH")
        .map(|value| format!("env \"LD_LIBRARY_PATH={value}\" "))
        .unwrap_or_default();
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=Decentraland Bevy Explorer (deep links)\nExec={env}\"{}\" %u\nNoDisplay=true\nMimeType=x-scheme-handler/{SCHEME};\n",
        exe.display()
    ))
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    let applications = applications_dir().ok_or_else(|| anyhow::anyhow!("no home directory"))?;
    std::fs::create_dir_all(&applications)?;
    std::fs::write(applications.join(DESKTOP_FILE), registration()?)?;
    let output = std::process::Command::new("xdg-mime")
        .args([
            "default",
            DESKTOP_FILE,
            &format!("x-scheme-handler/{SCHEME}"),
        ])
        .output()
        .map_err(|e| {
            anyhow::anyhow!(
                "xdg-mime (xdg-utils) is needed to register the {SCHEME}:// handler: {e}"
            )
        })?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to register the {SCHEME}:// handler: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    // optional: refreshes the mime cache for desktops that read it
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications)
        .output();
    bevy::log::info!("registered {} as the {SCHEME}:// handler", DESKTOP_FILE);
    Ok(())
}

fn applications_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_local_dir().join("applications"))
}
