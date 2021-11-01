use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
pub enum ColorMode {
    True,
    EightBit,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug)]
pub struct VideoTrack {
    pub name: Option<String>,         // optional name for the track
    pub framerate: f64,               // what framerate should this be played at
    pub color_mode: ColorMode,        // what color mode does the track use
    pub compression: CompressionMode, // how is the track compressed
    pub height: u32, // height in pixels (divide by two to get line count for terminal)
    pub width: u32,  // width in pixels
    pub encode_time: u64, // unix timestamp of time of encoding start
    pub offset: u64, // position in file at which it starts
    pub length: u64, // position in file at which it ends,
    pub frame_lengths: Vec<u64>, // length of every frame
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SubtitleTrack {
    pub name: Option<String>,
    pub lang: Option<String>,
    #[serde(with = "SubtitleFormatDef")]
    pub format: SubtitleFormat, // format for the subtitles
    pub offset: u64, // position in file at which it starts
    pub length: u64, // position in file at which it ends,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VideoMetadata {
    pub video_tracks: Vec<VideoTrack>,
    pub subtitle_tracks: Vec<SubtitleTrack>,
}
