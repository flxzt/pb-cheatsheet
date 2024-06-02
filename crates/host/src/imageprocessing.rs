use pb_cheatsheet_com::CheatsheetImage;
use std::path::PathBuf;

pub(crate) async fn load_prepare_image(
    image: PathBuf,
    width: u32,
    height: u32,
) -> anyhow::Result<CheatsheetImage> {
    let img = image::io::Reader::open(image)?.decode()?;
    let data = img
        .resize_exact(width, height, image::imageops::FilterType::Gaussian)
        .into_luma8()
        .into_raw();
    Ok(CheatsheetImage {
        format: pb_cheatsheet_com::ImageFormat::Gray8,
        order: pb_cheatsheet_com::ByteOrder::BE,
        width,
        height,
        data,
    })
}
