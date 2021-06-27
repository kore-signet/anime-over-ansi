use srtlib::{Subtitle};
use std::io::{BufRead,Write};
use std::sync::RwLock;
use std::time::{Duration, Instant};

pub fn play<T, U>(
    reader: &mut T,
    framerate: f64,
    subtitles: &mut Vec<Subtitle>,
    writer_lock: &RwLock<U>,
) -> std::io::Result<()>
where
    T: BufRead,
    U: Write,
{

    let interval_micros = (1000000.0 / framerate) as u64;
    let interval = Duration::from_micros(interval_micros);

    let mut next_subtitle = subtitles.pop();

    print!("\x1B[2J\x1B[1;1H");

    let mut i = 0;
    let mut last = Instant::now();

    loop {
        let mut next_frame = vec![];

        let bytes_read = reader.read_until(b'.', &mut next_frame)?;

        if bytes_read == 0 {
            break;
        }

        let next_frame_s = String::from_utf8(next_frame).unwrap();

        let mut dest = writer_lock.write().unwrap();

        write!(&mut dest, "\x1B[1;1H")?;
        write!(&mut dest, "{}", next_frame_s)?;

        if let Some(ref next_s) = next_subtitle {
            let (sh, sm, ss, sms) = next_s.start_time.get();

            let start_time =
                Duration::new(sh as u64 * 3600 + sm as u64 * 60 + ss as u64, sms as u32);

            let current = Duration::from_micros((i as u64 * interval_micros) as u64);

            if current >= start_time {
                write!(&mut dest, "\x1B[0m\x1B[0J{}", &next_s.text.replace("\n", " "))?;
            }

            let (eh, em, es, ems) = next_s.end_time.get();
            let end_time = Duration::new(eh as u64 * 3600 + em as u64 * 60 + es as u64, ems as u32);

            if current >= end_time {
                next_subtitle = subtitles.pop();
            }
        }

        i += 1;

        let sleep_for = interval.checked_sub(last.elapsed());
        last = Instant::now();

        if let Some(sl) = sleep_for {
            std::thread::sleep(sl);
        }
    }

    Ok(())
}
