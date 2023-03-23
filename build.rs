use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(&["src/dcl_component/proto/*.proto"], &["src/"])?;
    Ok(())
}
