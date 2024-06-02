use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::{pixelcolor, prelude::*};
use pb_cheatsheet_com::{CheatsheetTags, CheatsheetsInfo, WmClassTags};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Default)]
pub(crate) struct Cheatsheets {
    /// Contains the cheatsheets.
    ///
    /// key: cheatsheet name
    /// value: tuple containing metadata and cheatsheet
    sheets: HashMap<String, (CheatsheetMetadata, Cheatsheet)>,
    /// Contains tags associated with focused window wm class
    wm_class_tags: HashMap<String, HashSet<String>>,
}

impl Cheatsheets {
    pub(crate) fn get_sheet_tags(&self) -> Vec<CheatsheetTags> {
        self.sheets
            .iter()
            .map(|(name, (metadata, _))| {
                let tags = metadata.tags.iter().cloned().collect();
                CheatsheetTags {
                    name: name.to_owned(),
                    tags,
                }
            })
            .collect()
    }

    pub(crate) fn get_wm_classes_tags(&self) -> Vec<WmClassTags> {
        self.wm_class_tags
            .iter()
            .map(|(wm_class, tags)| {
                let tags = tags.iter().cloned().collect();
                WmClassTags {
                    wm_class: wm_class.to_owned(),
                    tags,
                }
            })
            .collect()
    }

    pub(crate) fn get_info(&self) -> CheatsheetsInfo {
        CheatsheetsInfo {
            cheatsheets: self.get_sheet_tags(),
            wm_classes: self.get_wm_classes_tags(),
        }
    }

    pub(crate) fn sheets_iter(
        &self,
    ) -> impl Iterator<Item = (&String, &(CheatsheetMetadata, Cheatsheet))> {
        self.sheets.iter()
    }

    #[allow(unused)]
    pub(crate) fn sheets_mut(
        &mut self,
    ) -> impl Iterator<Item = (&String, &mut (CheatsheetMetadata, Cheatsheet))> {
        self.sheets.iter_mut()
    }

    pub(crate) fn insert_sheet(
        &mut self,
        sheet: Cheatsheet,
        name: String,
        tags: HashSet<String>,
    ) -> Option<(CheatsheetMetadata, Cheatsheet)> {
        self.sheets
            .insert(name, (CheatsheetMetadata { tags }, sheet))
    }

    pub(crate) fn remove_sheet(&mut self, name: &str) -> Option<(CheatsheetMetadata, Cheatsheet)> {
        self.sheets.remove(name)
    }

    pub(crate) fn wm_class_n_sheets(&self, wm_class: &str) -> usize {
        self.sheets_for_wm_class(wm_class).len()
    }

    pub(crate) fn add_sheet_tag(&mut self, name: &str, tag: String) -> anyhow::Result<bool> {
        let Some((metadata, _sheet)) = self.sheets.get_mut(name) else {
            return Err(anyhow::anyhow!("Cheatsheet with name '{name}' not found."));
        };
        Ok(metadata.tags.insert(tag))
    }

    pub(crate) fn remove_sheet_tag(&mut self, name: &str, tag: &str) -> anyhow::Result<bool> {
        let Some((metadata, _sheet)) = self.sheets.get_mut(name) else {
            return Err(anyhow::anyhow!("Cheatsheet with name '{name}' not found."));
        };
        Ok(metadata.tags.remove(tag))
    }

    pub(crate) fn add_wm_class_tag(&mut self, wm_class: &str, tag: String) -> bool {
        if let Some(tags) = self.wm_class_tags.get_mut(wm_class) {
            tags.insert(tag)
        } else {
            let mut tags = HashSet::new();
            tags.insert(tag);
            self.wm_class_tags.insert(wm_class.to_string(), tags);
            false
        }
    }

    pub(crate) fn remove_wm_class_tag(
        &mut self,
        wm_class: &str,
        tag: &str,
    ) -> anyhow::Result<bool> {
        let tags = self.wm_class_tags.get_mut(wm_class).ok_or_else(|| {
            anyhow::anyhow!("Removing tag from wm class '{wm_class}' failed, not present.")
        })?;
        Ok(tags.remove(tag))
    }

    #[allow(unused)]
    pub(crate) fn sheets_for_tag<'i>(
        &'i self,
        tag: &'i str,
    ) -> impl Iterator<Item = (&'i CheatsheetMetadata, &'i Cheatsheet)> {
        self.sheets
            .iter()
            .filter_map(move |(_name, (metadata, sheet))| {
                if metadata.tags.contains(tag) {
                    Some((metadata, sheet))
                } else {
                    None
                }
            })
    }

    pub(crate) fn sheets_for_wm_class<'i>(
        &'i self,
        wm_class: &'i str,
    ) -> Vec<(&'i CheatsheetMetadata, &'i Cheatsheet)> {
        let Some(wm_class_tags) = self.wm_class_tags.get(wm_class).map(|i| i.iter()) else {
            return Vec::new();
        };

        let found_sheets = self
            .sheets
            .iter()
            .filter_map(move |(_, (metadata, sheet))| {
                if metadata
                    .tags
                    .iter()
                    .any(|tag| wm_class_tags.clone().any(|t| t == tag))
                {
                    Some((metadata, sheet))
                } else {
                    None
                }
            });
        // TODO: implement a tag-priority sorting
        found_sheets.collect()
    }

    pub(crate) fn dispatch_save_to_path(
        &self,
        base_path: impl AsRef<Path>,
        file_save_tx: mpsc::UnboundedSender<(PathBuf, Vec<u8>)>,
    ) -> anyhow::Result<()> {
        let base_path = base_path.as_ref();

        for (name, (metadata, image)) in self.sheets.iter() {
            let cheatsheet_path = base_path.join(format!("{name}.cs"));
            let cheatsheet_data = bincode::serialize(image)?;
            file_save_tx.send((cheatsheet_path, cheatsheet_data))?;

            let metadata_path = base_path.join(format!("{name}-metadata.json"));
            let metadata_data = serde_json::to_vec(metadata)?;
            file_save_tx.send((metadata_path, metadata_data))?;
        }

        let wm_class_tags_path = base_path.join("wm_class_tags.json");
        let wm_class_tags_data = serde_json::to_vec(&self.wm_class_tags)?;
        file_save_tx.send((wm_class_tags_path, wm_class_tags_data))?;

        Ok(())
    }

    pub(crate) async fn load_from_path(base_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let base_path = base_path.as_ref();
        let mut sheets = HashMap::new();

        for entry in base_path.read_dir()? {
            let entry_path = entry?.path();
            if !entry_path.extension().map(|e| e == "cs").unwrap_or(false) {
                continue;
            }
            tracing::debug!("Loading cheatsheet from file '{}'", entry_path.display());
            let basename = entry_path
                .file_stem()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Cheatsheet file '{}' does not have file stem",
                        entry_path.display()
                    )
                })?
                .to_str()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Cheatsheet file '{}' stem not valid UTF-8",
                        entry_path.display()
                    )
                })?;

            let mut cheatsheet_data: Vec<u8> = Vec::new();
            fs::File::open(&entry_path)
                .await?
                .read_to_end(&mut cheatsheet_data)
                .await?;
            let cheatsheet: Cheatsheet = bincode::deserialize(&cheatsheet_data)?;

            let mut metadata_data: Vec<u8> = Vec::new();
            fs::File::open(base_path.join(format!("{basename}-metadata.json")))
                .await?
                .read_to_end(&mut metadata_data)
                .await?;
            let metadata: CheatsheetMetadata = serde_json::from_slice(&metadata_data)?;

            sheets.insert(basename.to_string(), (metadata, cheatsheet));
        }

        let mut wm_class_tags_data = Vec::new();
        fs::File::open(base_path.join("wm_class_tags.json"))
            .await?
            .read_to_end(&mut wm_class_tags_data)
            .await?;
        let wm_class_tags = serde_json::from_slice(&wm_class_tags_data)?;
        Ok(Self {
            sheets,
            wm_class_tags,
        })
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct CheatsheetMetadata {
    pub(crate) tags: HashSet<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Cheatsheet {
    pub(crate) image: pb_cheatsheet_com::CheatsheetImage,
}

impl embedded_graphics::Drawable for Cheatsheet {
    type Color = pixelcolor::Gray8;

    type Output = ();

    fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
    where
        D: DrawTarget<Color = Self::Color>,
    {
        let target_center = target.bounding_box().center();
        match (self.image.format, self.image.order) {
            (pb_cheatsheet_com::ImageFormat::Gray8, pb_cheatsheet_com::ByteOrder::LE) => {
                let raw_image = convert_image_to_eg_bw_le(&self.image);
                let image = Image::with_center(&raw_image, target_center);
                image.draw(target)?;
            }
            (pb_cheatsheet_com::ImageFormat::Gray8, pb_cheatsheet_com::ByteOrder::BE) => {
                let raw_image = convert_image_to_eg_bw_be(&self.image);
                let image = Image::with_center(&raw_image, target_center);
                image.draw(target)?;
            }
            _ => unimplemented!(),
        }
        Ok(())
    }
}

fn convert_image_to_eg_bw_le(
    image: &pb_cheatsheet_com::CheatsheetImage,
) -> ImageRaw<'_, pixelcolor::Gray8, pixelcolor::raw::LittleEndian> {
    embedded_graphics::image::ImageRaw::new(&image.data, image.width)
}

fn convert_image_to_eg_bw_be(
    image: &pb_cheatsheet_com::CheatsheetImage,
) -> ImageRaw<'_, pixelcolor::Gray8, pixelcolor::raw::BigEndian> {
    embedded_graphics::image::ImageRaw::new(&image.data, image.width)
}
