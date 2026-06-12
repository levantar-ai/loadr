use std::path::PathBuf;

use prost::Message as _;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/loadr/coordination/v1/coordination.proto");

    // Compile the proto with protox (pure Rust, no system protoc needed).
    let fds = protox::compile(
        ["proto/loadr/coordination/v1/coordination.proto"],
        ["proto"],
    )?;

    // Persist the file descriptor set for reflection/dynamic-codec use.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(
        out_dir.join("coordination_descriptor.bin"),
        fds.encode_to_vec(),
    )?;

    // Generate tonic client + server code from the descriptor set.
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_fds(fds)?;

    Ok(())
}
