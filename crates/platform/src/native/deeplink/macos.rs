use std::path::PathBuf;

use super::{Handler, SCHEME};

const LAUNCHER_BUNDLE_ID: &str = "com.decentraland.launcherlite";

/// Only an app bundle can claim a scheme, so the check is "is the launcher installed"
pub fn handler() -> Handler {
    let in_applications = ["/Applications/Decentraland.app"]
        .into_iter()
        .map(PathBuf::from)
        .chain(
            directories::BaseDirs::new()
                .map(|dirs| dirs.home_dir().join("Applications/Decentraland.app")),
        )
        .any(|path| path.exists());
    if in_applications {
        return Handler::Other;
    }
    let installed = std::process::Command::new("mdfind")
        .arg(format!(
            "kMDItemCFBundleIdentifier == '{LAUNCHER_BUNDLE_ID}'"
        ))
        .output()
        .map(|output| output.status.success() && !output.stdout.trim_ascii().is_empty())
        .unwrap_or(false);
    if installed {
        Handler::Other
    } else {
        Handler::None
    }
}

pub fn registration() -> Result<String, anyhow::Error> {
    anyhow::bail!("no scheme registration on this platform")
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    anyhow::bail!(
        "nothing on this mac handles {SCHEME}:// links. Install the Decentraland launcher (https://decentraland.org/download) and try again"
    )
}
