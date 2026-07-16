fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "proto/veoveo/recording/ingest/v1/ingest.proto";
    println!("cargo:rerun-if-changed={proto}");

    let mut config = prost_build::Config::new();
    config.protoc_executable(protoc_bin_vendored::protoc_bin_path()?);
    config.compile_protos(&[proto], &["proto"])?;
    Ok(())
}
