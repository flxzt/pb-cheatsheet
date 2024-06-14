use pb_cheatsheet_com::CheatsheetImage;
use std::path::PathBuf;

/// In clockwise direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(unused)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Rotate {
    Rotate0Deg,

    Rotate90Deg,
    Rotate180Deg,
    Rotate270Deg,
}

pub(crate) async fn load_prepare_image(
    image: PathBuf,
    width: u32,
    height: u32,
    rotate: Rotate,
    invert: bool,
) -> anyhow::Result<CheatsheetImage> {
    let mut img = image::io::Reader::open(image)?.decode()?;
    if invert {
        img.invert();
    }
    let img = match rotate {
        Rotate::Rotate0Deg => img,
        Rotate::Rotate90Deg => img.rotate90(),
        Rotate::Rotate180Deg => img.rotate180(),
        Rotate::Rotate270Deg => img.rotate270(),
    };
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
