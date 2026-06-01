use std::{collections::HashSet, io::Result, process::Command};

fn main() -> Result<()> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    let git_hash = String::from_utf8(output.stdout).unwrap();
    println!("cargo:rustc-env=BEVY_EXPLORER_VERSION={git_hash}");

    modified()?;

    Ok(())
}

fn modified() -> Result<()> {
    let diff = Command::new("git")
        .args(["diff", "--name-only", "-z"])
        .output()?;
    if !diff.status.success() {
        return Err(std::io::Error::other(format!("{}", diff.status)));
    }
    let mut modifications = String::from_utf8(diff.stdout)
        .unwrap()
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();

    let diff = Command::new("git")
        .args(["diff", "--staged", "--name-only", "-z"])
        .output()?;
    if !diff.status.success() {
        return Err(std::io::Error::other(format!("{}", diff.status)));
    }
    modifications.extend(
        String::from_utf8(diff.stdout)
            .unwrap()
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(ToOwned::to_owned),
    );

    let change_to_build_rs = modifications.contains("build.rs");
    let change_to_workspace =
        modifications.contains("Cargo.toml") || modifications.contains("Cargo.lock");
    let changes_to_source = modifications
        .iter()
        .any(|path| path.starts_with("src/") || path.starts_with("crates/"));
    let modified = change_to_build_rs || change_to_workspace || changes_to_source;

    println!(
        "cargo:rustc-env=BEVY_EXPLORER_LOCAL_MODIFICATION={}",
        if modified { "true" } else { "false" }
    );
    Ok(())
}
