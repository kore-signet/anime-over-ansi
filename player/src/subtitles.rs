use container::bytes_hacking;

use crate::PacketFilterTransformer;

pub struct SSAParser<T: Fn(&substation::Entry) -> bool> {
    header: Vec<String>,
    show_entry_name: bool,
    filter: T,
}

impl<T: Fn(&substation::Entry) -> bool> SSAParser<T> {
    pub fn with_filter(
        mut codec_private: String,
        show_entry_name: bool,
        filter: T,
    ) -> Option<SSAParser<T>> {
        let mut header: Vec<String> = Vec::new();
        while let Ok((input, (_, section))) = substation::parser::section_with_input(&codec_private)
        {
            codec_private = input.trim_start().to_owned();

            if let Some(h) = section.as_event_header() {
                header = h.clone();
            }
        }

        if header.is_empty() {
            return None;
        }

        Some(SSAParser {
            header,
            filter,
            show_entry_name,
        })
    }
}

impl<T: Fn(&substation::Entry) -> bool> PacketFilterTransformer for SSAParser<T> {
    fn filter_map_packet(
        &mut self,
        mut packet: container::packet::Packet<bytes::Bytes>,
    ) -> Option<container::packet::Packet<bytes::Bytes>> {
        let str_data = String::from_utf8_lossy(&packet.data);

        let (_, entry) = substation::parser::subtitle(&str_data, &self.header)
            .ok()
            .filter(|(_, b)| (self.filter)(b))?;

        let mut text_bytes = Vec::new();

        if self.show_entry_name {
            if let Some(entry_name) = entry.name {
                text_bytes.extend_from_slice(entry_name.as_bytes());
                text_bytes.extend_from_slice(b": ");
            }
        }

        text_bytes.append(
            &mut substation::parser::text_line(&entry.text)
                .ok()?
                .1
                .into_iter()
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
        );

        packet.data = unsafe { bytes_hacking::bytesmut_from_vec(text_bytes) }.freeze();

        Some(packet)
    }
}
