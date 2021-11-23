use derive_builder::Builder;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ColorMode {
    True,
    EightBit,
}

impl ColorMode {
    pub fn byte_size(&self) -> usize {
        match self {
            ColorMode::True => 3,
            ColorMode::EightBit => 1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum CompressionMode {
    None = 0,
    Zstd = 1,
}

use subparse::SubtitleFormat;
#[derive(Serialize, Deserialize)]
#[serde(remote = "SubtitleFormat")]
enum SubtitleFormatDef {
    SubRip,
    SubStationAlpha,
    VobSubIdx,
    VobSubSub,
    MicroDVD,
}

#[derive(Builder, Serialize, Deserialize, Debug, Clone)]
pub struct VideoTrack {
    #[builder(default)]
    pub name: Option<String>, // optional name for the track
    pub color_mode: ColorMode,        // what color mode does the track use
    pub compression: CompressionMode, // how is the track compressed
    pub height: u32, // height in pixels (divide by two to get line count for terminal)
    pub width: u32,  // width in pixels
    pub encode_time: u64, // unix timestamp of time of encoding start
    pub index: u32,
}

#[derive(Serialize, Deserialize, Debug, Builder, Clone)]
pub struct SubtitleTrack {
    #[builder(default)]
    pub name: Option<String>,
    #[builder(default)]
    pub lang: Option<String>,
    #[serde(with = "SubtitleFormatDef")]
    pub format: SubtitleFormat, // format for the subtitles
    pub index: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VideoMetadata {
    pub video_tracks: Vec<VideoTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}
