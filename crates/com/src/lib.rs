use core::fmt::Display;
use std::collections::HashSet;
use std::fmt::Debug;

pub const RPC_PORT: u16 = 50051;

#[tarpc::service]
pub trait World {
    async fn focused_window(info: FocusedWindowInfo);
    async fn get_info() -> Info;
    async fn upload_cheatsheet(image: CheatsheetImage, name: String, tags: HashSet<String>);
    async fn remove_cheatsheet(name: String);
    async fn upload_screenshot(screenshot: CheatsheetImage, name: Option<String>);
    async fn clear_screenshot();
    async fn add_cheatsheet_tags(name: String, tags: HashSet<String>);
    async fn remove_cheatsheet_tags(name: String, either: TagsEither);
    async fn add_wm_class_tags(wm_class: String, tags: HashSet<String>);
    async fn remove_wm_class_tags(wm_class: String, either: TagsEither);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TagsEither {
    Tags(HashSet<String>),
    All,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct FocusedWindowInfo {
    pub title: String,
    pub wm_class: String,
    pub wm_class_instance: String,
    pub pid: u64,
    pub focus: bool,
}

impl Default for FocusedWindowInfo {
    fn default() -> Self {
        Self {
            title: "".to_string(),
            wm_class: "".to_string(),
            wm_class_instance: "".to_string(),
            pid: u64::MAX,
            focus: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Default, serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub screen_width: u32,
    pub screen_height: u32,
    pub screen_orientation: ScreenOrientation,
    pub cheatsheets: Vec<CheatsheetTags>,
    pub wm_classes: Vec<WmClassTags>,
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct CheatsheetTags {
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
pub struct WmClassTags {
    pub wm_class: String,
    pub tags: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum ImageFormat {
    Gray8,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[non_exhaustive]
pub enum ByteOrder {
    LittleEndian,
    BigEndian,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CheatsheetImage {
    pub format: ImageFormat,
    pub byte_order: ByteOrder,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Debug for CheatsheetImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheatsheetImage")
            .field("format", &self.format)
            .field("byte_order", &self.byte_order)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("data", &"- skip -".to_string())
            .finish()
    }
}

impl Default for CheatsheetImage {
    fn default() -> Self {
        Self {
            format: ImageFormat::Gray8,
            byte_order: ByteOrder::LittleEndian,
            width: 0,
            height: 0,
            data: Vec::default(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, Default, serde:: Serialize, serde::Deserialize,
)]
pub enum ScreenOrientation {
    #[default]
    Portrait0Deg,
    Landscape90Deg,
    Portrait180Deg,
    Landscape270Deg,
}

impl Display for ScreenOrientation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScreenOrientation::Portrait0Deg => write!(f, "Portrait0Deg"),
            ScreenOrientation::Landscape90Deg => write!(f, "Landscape90Deg"),
            ScreenOrientation::Portrait180Deg => write!(f, "Portrait180Deg"),
            ScreenOrientation::Landscape270Deg => write!(f, "Landscape270Deg"),
        }
    }
}
