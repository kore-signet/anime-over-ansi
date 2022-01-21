// use anime_telnet::encoding::{EncodedPacket, PacketTransformer};
// use midly::{MetaMessage, MidiMessage, TrackEvent, TrackEventKind};
// use std::time::Duration;

// pub struct MidiEncoder {
//     header: midly::Header,
//     stream_index: u32,
//     tick_time: Duration,
//     ticks_per_beat: u32,
//     tick_counter: u64,
// }

// impl<'a> MidiEncoder {
//     pub fn new(header: midly::Header, stream_index: u32) -> MidiEncoder {
//         let (tick_time, ticks_per_beat) = match header.timing {
//             midly::Timing::Timecode(fps, subframe) => (
//                 Duration::from_secs_f32(1.0 / fps.as_f32() / subframe as f32),
//                 1,
//             ),
//             midly::Timing::Metrical(ticks_per_beat) => {
//                 (
//                     Duration::from_secs(ticks_per_beat.as_int() as u64),
//                     ticks_per_beat.as_int() as u32,
//                 ) // placeholder until we get a tempo message
//             }
//         };

//         MidiEncoder {
//             stream_index,
//             tick_time,
//             header,
//             ticks_per_beat,
//             tick_counter: 0,
//         }
//     }
// }

// impl<'a> PacketTransformer<'a> for MidiEncoder {
//     type Source = &'a TrackEvent<'a>;
//     fn encode_packet(&mut self, src: &Self::Source) -> Option<EncodedPacket> {
//         let track_event = src;
//         let current_tick = self.tick_counter + track_event.delta.as_int() as u64;

//         if let TrackEventKind::Meta(MetaMessage::Tempo(beat_timing)) = track_event.kind {
//             self.tick_time =
//                 Duration::from_micros(beat_timing.as_int() as u64 / self.ticks_per_beat as u64);
//             return None;
//         }

//         let pts = Duration::from_nanos(current_tick * self.tick_time.as_nanos() as u64);
//         self.tick_counter = current_tick;

//         if let Some(ev) = track_event.kind.as_live_event() {
//             let mut v: Vec<u8> = Vec::new();
//             ev.write_std(&mut v).unwrap();
//             Some(EncodedPacket::from_data(
//                 self.stream_index,
//                 pts,
//                 None,
//                 v,
//                 None,
//             ))
//         } else {
//             None
//         }
//     }
// }

fn main() {
    
}