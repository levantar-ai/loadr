use std::path::PathBuf;

use prost::Message as _;

fn main() -> anyhow::Result<()> {
    println!("cargo:rerun-if-changed=proto/echo.proto");

    // Compile the proto with protox (pure Rust, no system protoc needed).
    let fds = protox::compile(["proto/echo.proto"], ["proto"])?;

    // Persist the file descriptor set so the library can expose it for
    // dynamic-codec / reflection use by client tests.
    let out_dir = PathBuf::from(std::env::var("OUT_DIR")?);
    let fds_path = out_dir.join("echo_descriptor.bin");
    std::fs::write(&fds_path, fds.encode_to_vec())?;

    // Generate tonic client + server code from the descriptor set.
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_fds(fds)?;

    Ok(())
}
