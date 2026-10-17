// Compiles the test protos without protoc: protox builds the descriptor set,
// tonic-prost-build generates the server code from it.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fds = protox::compile(["proto/test.proto"], ["proto"])?;
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
    std::fs::write(
        out.join("test_descriptor.bin"),
        prost::Message::encode_to_vec(&fds),
    )?;
    tonic_prost_build::configure()
        .build_client(false)
        .compile_fds(fds)?;
    println!("cargo:rerun-if-changed=proto");
    Ok(())
}
