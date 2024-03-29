use derive_builder::Builder;

use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(TryFromPrimitive, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum ColorMode {
    True = 0,
    EightBit = 1,
}

impl ColorMode {
    pub fn byte_size(&self) -> usize {
        match self {
            ColorMode::True => 3,
            ColorMode::EightBit => 1,
        }
    }
}

impl fmt::Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ColorMode::True => write!(f, "true"),
            ColorMode::EightBit => write!(f, "eight-bit"),
        }
    }
}

#[derive(TryFromPrimitive, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
#[repr(u8)]
pub enum CompressionMode {
    None = 0,
    Zstd = 1,
}

impl fmt::Display for CompressionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompressionMode::None => write!(f, "none"),
            CompressionMode::Zstd => write!(f, "zstd"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubtitleFormat {
    SubRip,
    SubStationAlpha,
    Unknown(String),
}

impl SubtitleFormat {
    pub fn from_codec_name(codec: &str) -> SubtitleFormat {
        match codec {
            "srt" => SubtitleFormat::SubRip,
            "ssa" | "ass" => SubtitleFormat::SubStationAlpha,
            _ => SubtitleFormat::Unknown(codec.to_owned()),
        }
    }
}

impl fmt::Display for SubtitleFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubtitleFormat::SubStationAlpha => write!(f, "substation alpha (.ssa)"),
            SubtitleFormat::SubRip => write!(f, "subrip (.srt)"),
            SubtitleFormat::Unknown(ref s) => write!(f, "unsupported/unknown ({})", s),
        }
    }
}

#[derive(Builder, Serialize, Deserialize, Debug, Clone)]
pub struct VideoTrack {
    #[builder(default)]
    pub name: Option<String>, // optional name for the track
    pub color_mode: ColorMode, // what color mode does the track use
    pub height: u32,           // height in pixels (divide by two to get line count for terminal)
    pub width: u32,            // width in pixels
    #[builder(default)]
    pub codec_private: Option<Vec<u8>>,
    pub index: u16,
}

#[derive(Serialize, Deserialize, Debug, Builder, Clone)]
pub struct SubtitleTrack {
    #[builder(default)]
    pub name: Option<String>,
    #[builder(default)]
    pub lang: Option<String>,
    pub format: SubtitleFormat, // format for the subtitles
    #[builder(default)]
    pub codec_private: Option<Vec<u8>>,
    pub index: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Attachment {
    Binary(Vec<u8>),
    Midi(Vec<u8>),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VideoMetadata {
    pub video_tracks: Vec<VideoTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub compression: CompressionMode,
}
