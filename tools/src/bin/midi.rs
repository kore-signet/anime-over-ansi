use anime_telnet::encoding::PacketTransformer;
use anime_telnet_encoder::midi::MidiEncoder;
use fluidlite::{Settings, Synth};
use midly::live::LiveEvent;
use midly::Smf;
use rodio::source::{self, Source};
use rodio::{Decoder, OutputStream, Sink};
use std::time::{Duration, Instant};
use synthrs::synthesizer::make_samples_from_midi_file;
use synthrs::wave;

fn main() {
    let samples: Vec<i16> =
        synthrs::synthesizer::quantize_samples::<i16>(&synthrs::synthesizer::peak_normalize(
            &make_samples_from_midi_file(wave::sine_wave, 44_100, false, "./hectopascal.mid")
                .unwrap(),
        ));

    let settings = Settings::new().unwrap();
    let synth = Synth::new(settings).unwrap();
    synth.sfload("./6868.sf2", true).unwrap();
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    sink.append(rodio::buffer::SamplesBuffer::new(1, 44_100, samples));
    std::thread::sleep(std::time::Duration::from_secs(100));
    // let (queue_input, queue_output) = rodio::queue::queue(true);
    // sink.append(queue_output);
    // let mut samples = [0f32; 44100 * 2];

    // let start = Instant::now();
    // for packet in packets {
    //     let midi_event = LiveEvent::parse(&packet.data).unwrap();
    //     if let Some(to_wait) = packet.time.checked_sub(start.elapsed()) {
    //         std::thread::sleep(to_wait);
    //     }

    //     if let LiveEvent::Midi { channel, message } = midi_event {
    //         use midly::MidiMessage::*;
    //         match message {
    //             NoteOff { key, .. } => {
    //                 synth.note_off(channel.as_int() as u32, key.as_int() as u32);
    //             }
    //             NoteOn { key, vel } => {
    //                 synth.note_on(channel.as_int() as u32, key.as_int() as u32, vel.as_int() as u32);
    //             }
    //             Aftertouch { key, vel } => {
    //                 synth.key_pressure(channel.as_int() as u32, key.as_int() as u32, vel.as_int() as u32);
    //             }
    //             Controller { controller, value } => {
    //                 synth.cc(channel.as_int() as u32, controller.as_int() as u32, value.as_int() as u32);
    //             }
    //             ProgramChange { program } => {
    //                 synth.program_change(channel.as_int() as u32, program.as_int() as u32);
    //             }
    //             _ => {}
    //         }
    //     }

    //     synth.write(&mut samples[..]).unwrap();
    //     queue_input.append(rodio::buffer::SamplesBuffer::new(1, 44100, samples.to_vec()));
    // }
}
