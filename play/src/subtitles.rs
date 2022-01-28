use anime_telnet::encoding::{EncodedPacket, PacketDecoder};
use anime_telnet::subtitles::SSAFilter;
use std::time::Duration;

pub struct SubtitlePacket {
    pub start: Duration,
    pub end: Duration,
    pub payload: Vec<u8>,
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
                return render_ssa(entry);
            }

            None
        })
    }
}

/// A decoder that transforms packets into subrip subtitles.
pub struct SRTDecoder;

impl PacketDecoder for SRTDecoder {
    type Output = SubtitlePacket;

    fn decode_packet(&mut self, src: EncodedPacket) -> Option<Self::Output> {
        let time = src.time;
        let end = src.time + src.duration.unwrap();
        String::from_utf8(src.data)
            .ok()
            .map(|s| render_srt(time, end, s))
    }
}

pub fn render_srt(start: Duration, end: Duration, text: String) -> SubtitlePacket {
    SubtitlePacket {
        payload: format!("\x1B[2K {}", text).into_bytes(),
        start,
        end,
    }
}

pub fn render_ssa(entry: substation::Entry) -> Option<SubtitlePacket> {
    substation::parser::text_line(&entry.text)
        .ok()
        .map(|(_, text)| {
            let bytes = [
                b"\x1B[2K ".to_vec(),
                text.into_iter()
                    .filter_map(|v| {
                        if let substation::TextSection::Text(s) = v {
                            Some(s)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("")
                    .replace("\\N", "")
                    .into_bytes(),
            ]
            .concat();
            SubtitlePacket {
                start: entry.start.unwrap(),
                end: entry.end.unwrap(),
                payload: bytes,
            }
        })
}
