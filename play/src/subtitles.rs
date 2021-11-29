use anime_telnet::encoding::{EncodedPacket, PacketDecoder};
use anime_telnet::subtitles::SSAFilter;

pub enum SubtitlePacket {
    SSAEntry(substation::Entry),
    SRTEntry(subrip::Entry),
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
