use anime_telnet::{
    encoding::{EncodedPacket, PacketDecoder, PacketTransformer},
    metadata::SubtitleTrack,
};
use cyanotype::streams::SubtitlePacket;
use futures::stream::{self, Stream, StreamExt};
use std::pin::Pin;
use tokio::fs;
use tokio::io::{self, AsyncReadExt};

/// Filters for SSA subtitles.
#[derive(Default)]
pub struct SSAFilter {
    pub layers: Vec<isize>,
    pub styles: Vec<String>,
}

impl SSAFilter {
    pub fn check(&self, entry: &substation::Entry) -> bool {
        (self.layers.is_empty()
            || entry
                .layer
                .as_ref()
                .map(|v| self.layers.contains(v))
                .unwrap_or(false))
            && (self.styles.is_empty()
                || entry
                    .style
                    .as_ref()
                    .map(|v| self.styles.contains(v))
                    .unwrap_or(false))
    }
}

/// An encoder that transforms SSA subtitle entries into packets.
pub struct SSAEncoder {
    definitions: Vec<String>,
    stream_index: u32,
}

impl SSAEncoder {
    pub fn new(stream_index: u32, definitions: Vec<String>) -> SSAEncoder {
        SSAEncoder {
            stream_index,
            definitions,
        }
    }
}

impl PacketTransformer for SSAEncoder {
    type Source = SubtitlePacket;

    fn encode_packet(&self, src: &Self::Source) -> Option<EncodedPacket> {
        if let SubtitlePacket::SSAEntry(entry) = src {
            let mut line = String::new();
            for def in self.definitions.iter().map(|v| v.as_str()) {
                line += &(match def {
                    "Layer" => entry.layer.map(|v| v.to_string()),
                    "Style" => entry.style.clone(),
                    "Name" => entry.name.clone(),
                    "MarginL" => entry.margin_l.map(|v| v.to_string()),
                    "MarginR" => entry.margin_r.map(|v| v.to_string()),
                    "MarginV" => entry.margin_v.map(|v| v.to_string()),
                    "Effect" => entry.effect.clone(),
                    "ReadOrder" => entry.read_order.map(|v| v.to_string()),
                    "Text" => {
                        line += &entry.text;
                        break;
                    }
                    _ => continue,
                })
                .unwrap_or("".to_string());
                line += ",";
            }

            Some(EncodedPacket::from_data(
                self.stream_index,
                entry.start.unwrap(),
                entry.end.map(|v| v - entry.start.unwrap()),
                line.into_bytes(),
                None,
            ))
        } else {
            None
        }
    }
}

/// A decoder that transforms packets into SSA subtitles.
pub struct SSADecoder {
    definition_header: Vec<String>,
    filter: SSAFilter,
}

impl SSADecoder {
    pub fn new(definition_header: Vec<String>, filter: Option<SSAFilter>) -> SSADecoder {
        SSADecoder {
            definition_header,
            filter: filter.unwrap_or_default(),
        }
    }
}

impl PacketDecoder for SSADecoder {
    type Output = SubtitlePacket;

    fn decode_packet(&mut self, src: EncodedPacket) -> Option<Self::Output> {
        let time = src.time;
        let end = src.time + src.duration.unwrap();

        substation::parser::subtitle(
            &String::from_utf8(src.data).unwrap(),
            &self.definition_header,
        )
        .ok()
        .and_then(|(_, mut entry)| {
            entry.start = Some(time);
            entry.end = Some(end);

            if self.filter.check(&entry) {
                Some(SubtitlePacket::SSAEntry(entry))
            } else {
                None
            }
        })
    }
}

/// An encoder that transforms subrip subtitles into packets.
pub struct SRTEncoder {
    stream_index: u32,
}

impl SRTEncoder {
    pub fn new(stream_index: u32) -> SRTEncoder {
        SRTEncoder { stream_index }
    }
}

impl PacketTransformer for SRTEncoder {
    type Source = SubtitlePacket;

    fn encode_packet(&self, src: &Self::Source) -> Option<EncodedPacket> {
        if let SubtitlePacket::SRTEntry(entry) = src {
            Some(EncodedPacket::from_data(
                self.stream_index,
                entry.start,
                Some(entry.end),
                entry.text.as_bytes().to_vec(),
                None,
            ))
        } else {
            None
        }
    }
}

/// A decoder that transforms packets into subrip subtitles.
pub struct SRTDecoder {
    idx: u32,
}

impl SRTDecoder {
    pub fn new() -> SRTDecoder {
        SRTDecoder { idx: 0 }
    }
}

impl PacketDecoder for SRTDecoder {
    type Output = SubtitlePacket;

    fn decode_packet(&mut self, src: EncodedPacket) -> Option<Self::Output> {
        let time = src.time;
        let end = src.time + src.duration.unwrap();
        self.idx += 1;

        String::from_utf8(src.data).ok().map(|s| {
            SubtitlePacket::SRTEntry(subrip::Entry {
                text: s,
                start: time,
                end,
                index: self.idx,
            })
        })
    }
}

/// A subtitle encoder that simply passes data through.
pub struct PassthroughSubtitleEncoder {
    stream_index: u32,
}

impl PassthroughSubtitleEncoder {
    pub fn new(stream_index: u32) -> PassthroughSubtitleEncoder {
        PassthroughSubtitleEncoder { stream_index }
    }
}

impl PacketTransformer for PassthroughSubtitleEncoder {
    type Source = SubtitlePacket;

    fn encode_packet(&self, src: &Self::Source) -> Option<EncodedPacket> {
        if let SubtitlePacket::Raw { start, end, data } = src {
            Some(EncodedPacket::from_data(
                self.stream_index,
                *start,
                Some(*end),
                data.clone(),
                None,
            ))
        } else {
            None
        }
    }
}

/// Converts an SSA file into a subtitle track and packets.
pub async fn ssa_file_to_packets(
    mut f: fs::File,
    mut subtitle_track: SubtitleTrack,
) -> io::Result<(
    Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>,
    SubtitleTrack,
)> {
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;
    let mut header: Vec<String> = Vec::new();

    let codec_private = {
        let mut codec_private = String::new();

        while let Ok((input, (section_str, section))) =
            substation::parser::section_with_input(&contents)
        {
            codec_private += section_str.trim_end();
            codec_private += "\n\n";
            contents = input.trim_start().to_owned();

            if let Some(h) = section.as_event_header() {
                header = h.clone();
            }
        }

        codec_private
    };

    subtitle_track.codec_private = Some(codec_private.into_bytes());

    let encoder = SSAEncoder::new(
        subtitle_track.index,
        vec![
            "ReadOrder",
            "Layer",
            "Style",
            "Name",
            "MarginL",
            "MarginR",
            "MarginV",
            "Effect",
            "Text",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    );

    Ok((
        stream::iter(
            contents
                .lines()
                .filter_map(|l| substation::parser::subtitle(l, &header).ok().map(|v| v.1))
                .filter(|entry| entry.kind.is_none() || entry.kind.as_ref().unwrap() != "Comment")
                .enumerate()
                .filter_map(|(i, mut entry)| {
                    entry.read_order = Some(i as isize);
                    encoder
                        .encode_packet(&SubtitlePacket::SSAEntry(entry))
                        .map(Ok)
                })
                .collect::<Vec<io::Result<EncodedPacket>>>(),
        )
        .boxed_local(),
        subtitle_track,
    ))
}

/// Converts a subrip file into packets.
pub async fn srt_file_to_packets(
    mut f: fs::File,
    stream_index: u32,
) -> io::Result<Pin<Box<dyn Stream<Item = std::io::Result<EncodedPacket>>>>> {
    let mut contents = String::new();
    f.read_to_string(&mut contents).await?;
    let encoder = SRTEncoder::new(stream_index);

    Ok(stream::iter(
        subrip::entries(&contents)
            .unwrap()
            .1
            .into_iter()
            .filter_map(|entry| {
                encoder
                    .encode_packet(&SubtitlePacket::SRTEntry(entry))
                    .map(Ok)
            })
            .collect::<Vec<io::Result<EncodedPacket>>>(),
    )
    .boxed_local())
}
