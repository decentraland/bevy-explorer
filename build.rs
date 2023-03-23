use std::io::Result;
fn main() -> Result<()> {
    let components = ["billboard", "raycast", "raycast_result"];

    let sources = components
        .iter()
        .map(|component| {
            format!("src/dcl_component/proto/decentraland/sdk/components/{component}.proto")
        })
        .collect::<Vec<_>>();

    prost_build::compile_protos(&sources, &["src/dcl_component/proto/"])?;
    Ok(())
}
