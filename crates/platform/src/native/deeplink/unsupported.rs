use std::path::PathBuf;

use super::Handler;

pub fn handler() -> Handler {
    Handler::None
}

pub fn exe_path() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::current_exe()?)
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    anyhow::bail!("deep-link sign-in is not supported on this platform")
}
