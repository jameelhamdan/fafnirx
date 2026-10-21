fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("PROTOC", protoc_bin_vendored::protoc_bin_path()?);
    tonic_build::compile_protos("../../proto/fafnirx.proto")?;
    println!("cargo:rerun-if-changed=../../proto/fafnirx.proto");
    Ok(())
}
