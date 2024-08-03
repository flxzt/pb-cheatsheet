pub mod grpc;

use core::fmt::Display;
use grpc::pb_cheatsheet_proto;
use num_traits::{FromPrimitive, ToPrimitive};
use std::collections::HashSet;
use std::fmt::Debug;

pub const PB_GRPC_PORT: u16 = 51151;

#[derive(Debug, Clone)]
pub enum TagsEither {
    Tags(HashSet<String>),
    All,
}

impl From<TagsEither> for pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either {
    fn from(value: TagsEither) -> Self {
        match value {
            TagsEither::Tags(tags) => {
                pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either::Tags(
                    pb_cheatsheet_proto::Tags {
                        tags: tags.into_iter().collect(),
                    },
                )
            }
            TagsEither::All => pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either::All(
                pb_cheatsheet_proto::Empty {},
            ),
        }
    }
}

impl From<pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either> for TagsEither {
    fn from(value: pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either) -> Self {
        match value {
            pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either::Tags(tags) => {
                Self::Tags(tags.tags.into_iter().collect())
            }
            pb_cheatsheet_proto::remove_cheatsheet_tags_request::Either::All(_) => Self::All,
        }
    }
}

impl From<TagsEither> for pb_cheatsheet_proto::remove_wm_class_tags_request::Either {
    fn from(value: TagsEither) -> Self {
        match value {
            TagsEither::Tags(tags) => {
                pb_cheatsheet_proto::remove_wm_class_tags_request::Either::Tags(
                    pb_cheatsheet_proto::Tags {
                        tags: tags.into_iter().collect(),
                    },
                )
            }
            TagsEither::All => pb_cheatsheet_proto::remove_wm_class_tags_request::Either::All(
                pb_cheatsheet_proto::Empty {},
            ),
        }
    }
}

impl From<pb_cheatsheet_proto::remove_wm_class_tags_request::Either> for TagsEither {
    fn from(value: pb_cheatsheet_proto::remove_wm_class_tags_request::Either) -> Self {
        match value {
            pb_cheatsheet_proto::remove_wm_class_tags_request::Either::Tags(tags) => {
                Self::Tags(tags.tags.into_iter().collect())
            }
            pb_cheatsheet_proto::remove_wm_class_tags_request::Either::All(_) => Self::All,
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
    pub orientation: ScreenOrientation,
}

#[derive(Debug, Clone, Default)]
pub struct CheatsheetsInfo {
    pub cheatsheets: Vec<CheatsheetTags>,
    pub wm_classes: Vec<WmClassTags>,
}

impl From<pb_cheatsheet_proto::CheatsheetsInfo> for CheatsheetsInfo {
    fn from(value: pb_cheatsheet_proto::CheatsheetsInfo) -> Self {
        Self {
            cheatsheets: value.cheatsheets.into_iter().map(|v| v.into()).collect(),
            wm_classes: value.wm_classes.into_iter().map(|v| v.into()).collect(),
        }
    }
}

impl From<CheatsheetsInfo> for pb_cheatsheet_proto::CheatsheetsInfo {
    fn from(value: CheatsheetsInfo) -> Self {
        Self {
            cheatsheets: value.cheatsheets.into_iter().map(|v| v.into()).collect(),
            wm_classes: value.wm_classes.into_iter().map(|v| v.into()).collect(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CheatsheetTags {
    pub name: String,
    pub tags: Vec<String>,
}

impl From<CheatsheetTags> for pb_cheatsheet_proto::CheatsheetTags {
    fn from(value: CheatsheetTags) -> Self {
        Self {
            name: value.name,
            tags: Some(pb_cheatsheet_proto::Tags { tags: value.tags }),
        }
    }
}

impl From<pb_cheatsheet_proto::CheatsheetTags> for CheatsheetTags {
    fn from(value: pb_cheatsheet_proto::CheatsheetTags) -> Self {
        Self {
            name: value.name,
            tags: value.tags.map(|t| t.tags).unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct WmClassTags {
    pub wm_class: String,
    pub tags: Vec<String>,
}

impl From<pb_cheatsheet_proto::WmClassTags> for WmClassTags {
    fn from(value: pb_cheatsheet_proto::WmClassTags) -> Self {
        Self {
            wm_class: value.wm_class,
            tags: value.tags.map(|t| t.tags).unwrap_or_default(),
        }
    }
}

impl From<WmClassTags> for pb_cheatsheet_proto::WmClassTags {
    fn from(value: WmClassTags) -> Self {
        Self {
            wm_class: value.wm_class,
            tags: Some(pb_cheatsheet_proto::Tags { tags: value.tags }),
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    num_derive::FromPrimitive,
    num_derive::ToPrimitive,
)]
#[non_exhaustive]
pub enum ImageFormat {
    Gray8 = 0,
}

impl TryFrom<i32> for ImageFormat {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::from_i32(value)
            .ok_or_else(|| anyhow::anyhow!("'ImageFormat' try from i32 with value '{value}'"))
    }
}

impl TryFrom<ImageFormat> for i32 {
    type Error = anyhow::Error;

    fn try_from(value: ImageFormat) -> Result<Self, Self::Error> {
        value
            .to_i32()
            .ok_or_else(|| anyhow::anyhow!("i32 try from 'ImageFormat' with value '{value:?}'"))
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    num_derive::FromPrimitive,
    num_derive::ToPrimitive,
)]
#[non_exhaustive]
pub enum ByteOrder {
    LE = 0,
    BE,
}

impl TryFrom<i32> for ByteOrder {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::from_i32(value).ok_or_else(|| {
            anyhow::anyhow!("'ByteOrder' try from failed for i32 with value '{value}'")
        })
    }
}

impl TryFrom<ByteOrder> for i32 {
    type Error = anyhow::Error;

    fn try_from(value: ByteOrder) -> Result<Self, Self::Error> {
        value
            .to_i32()
            .ok_or_else(|| anyhow::anyhow!("i32 try from 'ByteOrder' with value '{value:?}'"))
    }
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CheatsheetImage {
    pub format: ImageFormat,
    pub order: ByteOrder,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Debug for CheatsheetImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheatsheetImage")
            .field("format", &self.format)
            .field("order", &self.order)
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
            order: ByteOrder::LE,
            width: 0,
            height: 0,
            data: Vec::default(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, PartialOrd, num_derive::FromPrimitive, num_derive::ToPrimitive,
)]
pub enum ScreenOrientation {
    Portrait0Deg = 0,
    Landscape90Deg,
    Portrait180Deg,
    Landscape270Deg,
}

impl Default for ScreenOrientation {
    fn default() -> Self {
        Self::Portrait0Deg
    }
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

impl TryFrom<i32> for ScreenOrientation {
    type Error = anyhow::Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        Self::from_i32(value)
            .ok_or_else(|| anyhow::anyhow!("'ScreenSize' try from i32 with value '{value}'"))
    }
}

impl TryFrom<ScreenOrientation> for i32 {
    type Error = anyhow::Error;

    fn try_from(value: ScreenOrientation) -> Result<Self, Self::Error> {
        value
            .to_i32()
            .ok_or_else(|| anyhow::anyhow!("i32 try from 'ScreenSize' with value '{value}'"))
    }
}
