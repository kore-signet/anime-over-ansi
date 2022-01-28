use anime_telnet::metadata::Attachment;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use synthrs::midi;
use synthrs::synthesizer::*;
use synthrs::wave;

#[allow(dead_code)]
pub struct MidiPlayer {
    pub sink: Sink,
    inner_outstream: OutputStream,
    inner_handle: OutputStreamHandle,
}

impl MidiPlayer {
    pub fn new(attachments: &[Attachment]) -> MidiPlayer {
        let audio_samples: Vec<Vec<i16>> = attachments
            .iter()
            .filter_map(|v| {
                if let Attachment::Midi(bytes) = v {
                    let mut cursor = std::io::Cursor::new(bytes);
                    let track = midi::read_midi(&mut cursor).unwrap();
                    let samples =
                        make_samples_from_midi(wave::sine_wave, 44_100, true, track).unwrap();
                    Some(quantize_samples::<i16>(&peak_normalize(&samples)))
                } else {
                    None
                }
            })
            .collect();

        let (stream, stream_handle) = OutputStream::try_default().unwrap();
        let sink = Sink::try_new(&stream_handle).unwrap();
        sink.pause();
        for track in audio_samples {
            let buffer = rodio::buffer::SamplesBuffer::new(1, 44_100, track);
            sink.append(buffer);
        }

        MidiPlayer {
            sink,
            inner_handle: stream_handle,
            inner_outstream: stream,
        }
    }

    pub fn play(&self) {
        self.sink.play();
    }
}
