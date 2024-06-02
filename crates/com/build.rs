fn main() -> anyhow::Result<()> {
    tonic_build::compile_protos("proto/pb_cheatsheet.proto")?;
    Ok(())
}
