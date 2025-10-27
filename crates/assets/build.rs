use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

fn visit_dirs(writer: &mut BufWriter<File>, dir: &Path, base_dir: &Path) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(writer, &path, base_dir)?;
            } else if path.is_file() {
                let relative_path = path.strip_prefix(base_dir).unwrap();
                let relative_path = relative_path.to_string_lossy().replace('\\', "/");
                let full_path = path.canonicalize()?;
                let full_path = full_path.to_string_lossy().replace('\\', "\\\\");

                writeln!(writer, "    embedded.insert_asset(PathBuf::default(), Path::new(\"{relative_path}\"), include_bytes!(\"{full_path}\"));")?;
            }
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    // The directory to scan for files.
    const SOURCE_DIR: &str = "src/assets";
    let source_path = Path::new(SOURCE_DIR);

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_asset_embedding.rs");
    let mut writer = BufWriter::new(File::create(dest_path).unwrap());

    println!("cargo:rerun-if-changed={SOURCE_DIR}");

    writeln!(&mut writer, "use std::path::{{Path, PathBuf}};")?;
    writeln!(
        &mut writer,
        "pub fn embed_assets(embedded: &mut bevy::asset::io::embedded::EmbeddedAssetRegistry) {{"
    )?;

    visit_dirs(&mut writer, source_path, source_path)?;

    writeln!(&mut writer, "}}")?;

    Ok(())
}
