use super::Handler;

pub fn handler() -> Handler {
    Handler::None
}

pub fn registration() -> Result<String, anyhow::Error> {
    anyhow::bail!("no scheme registration on this platform")
}

pub fn register_handler() -> Result<(), anyhow::Error> {
    anyhow::bail!("deep-link sign-in is not supported on this platform")
}
