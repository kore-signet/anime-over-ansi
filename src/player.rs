use std::io::{Read, Write};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use subparse::SubtitleEntry;

pub fn play<T, U>(
    reader: &mut T,
    framerate: f64,
    frame_lengths: Vec<u64>,
    subtitles: &mut Vec<SubtitleEntry>,
    writer_lock: &RwLock<U>,
) -> std::io::Result<()>
where
    T: Read,
    U: Write,
{
    let interval_micros = (1000000.0 / framerate) as u64;
    let interval = Duration::from_micros(interval_micros);

    let mut next_subtitle = subtitles.pop();

    print!("\x1B[2J\x1B[1;1H");

    let mut i = 0;
    let mut last = Instant::now();

    while i < frame_lengths.len() - 1 {
        let mut next_frame = vec![0; frame_lengths[i] as usize];

        reader.read_exact(&mut next_frame)?;

        let next_frame_s = String::from_utf8(next_frame).unwrap();

        let mut dest = writer_lock.write().unwrap();

        write!(&mut dest, "\x1B[1;1H")?;
        write!(&mut dest, "{}", next_frame_s)?;

        if let Some(ref next_s) = next_subtitle {
            let start_time = Duration::from_millis(next_s.timespan.start.abs().msecs() as u64);

            let current = Duration::from_micros((i as u64 * interval_micros) as u64);

            if current >= start_time {
                write!(
                    &mut dest,
                    "\x1B[0m\x1B[0J{}",
                    &next_s.line.as_ref().unwrap().replace("\n", " ")
                )?;
            }

            let end_time = Duration::from_millis(next_s.timespan.end.abs().msecs() as u64);

            if current >= end_time {
                next_subtitle = subtitles.pop();
            }
        }

        i += 1;

        let sleep_for = interval.checked_sub(last.elapsed());

        if let Some(sl) = sleep_for {
            std::thread::sleep(sl);
        }

        last = Instant::now();
    }

    Ok(())
}
