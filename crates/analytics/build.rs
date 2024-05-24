use std::io::Result;
use std::process::Command;
fn main() -> Result<()> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=BEVY_EXPLORER_VERSION={}", git_hash);
    Ok(())
}
