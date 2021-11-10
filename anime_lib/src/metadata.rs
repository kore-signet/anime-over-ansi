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

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Hash)]
pub enum CompressionMode {
    None,
    Zstd,
    ZstdDict(String), // base64 encoded
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
    pub framerate: f64,               // what framerate should this be played at
    pub color_mode: ColorMode,        // what color mode does the track use
    pub compression: CompressionMode, // how is the track compressed
    pub height: u32, // height in pixels (divide by two to get line count for terminal)
    pub width: u32,  // width in pixels
    pub encode_time: u64, // unix timestamp of time of encoding start
    #[builder(default)]
    pub offset: u64, // position in file at which it starts
    #[builder(default)]
    pub length: u64, // position in file at which it ends,
    #[builder(default)]
    pub frame_lengths: Vec<u64>, // length of every frame
    #[builder(default)]
    pub frame_hashes: Vec<u32>, // adler32 hash of every frame
    #[builder(default)]
    pub frame_times: Vec<i64>, // time frame should be displayed at, in nanoseconds
}

#[derive(Serialize, Deserialize, Debug, Builder, Clone)]
pub struct SubtitleTrack {
    #[builder(default)]
    pub name: Option<String>,
    #[builder(default)]
    pub lang: Option<String>,
    #[serde(with = "SubtitleFormatDef")]
    pub format: SubtitleFormat, // format for the subtitles
    #[builder(default)]
    pub offset: u64, // position in file at which it starts
    #[builder(default)]
    pub length: u64, // position in file at which it ends,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VideoMetadata {
    pub video_tracks: Vec<VideoTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}
