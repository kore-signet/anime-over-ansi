#![allow(unused_assignments)]

use std::pin::Pin;

use simd_adler32::adler32;
use subparse::SubtitleEntry;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::broadcast,
    time::{sleep_until, Duration, Instant},
};

pub async fn play<T>(
    mut reader: Pin<&mut T>,
    frame_lengths: Vec<u64>,
    frame_hashes: Vec<u32>,
    frame_times: Vec<i64>,
    subtitles: &mut Vec<SubtitleEntry>,
    tx: broadcast::Sender<Vec<u8>>,
) -> anyhow::Result<()>
where
    T: AsyncRead,
{
    let start = Instant::now();
    let mut current = Duration::from_nanos(0);
    let mut next_subtitle = subtitles.pop();

    // print!("\x1B[2J\x1B[1;1H");

    let mut i = 0;

    while i < frame_lengths.len() - 1 {
        current = Duration::from_nanos(frame_times[i] as u64);

        let mut next_frame = vec![0; frame_lengths[i] as usize];
        reader.read_exact(&mut next_frame).await?;
        let hash = adler32(&next_frame.as_slice());
        if hash != frame_hashes[i] {
            panic!("detected corrupted data at frame {}", i);
        }

        tx.send(b"\x1B[1;1H".to_vec())?;
        tx.send(next_frame)?;

        if let Some(ref next_s) = next_subtitle {
            let start_time = Duration::from_millis(next_s.timespan.start.abs().msecs() as u64);

            if current >= start_time {
                tx.send(b"\x1B[0m\x1B[0J".to_vec())?;
                tx.send(
                    next_s
                        .line
                        .as_ref()
                        .unwrap()
                        .replace("\n", " ")
                        .as_bytes()
                        .to_vec(),
                )?;
            }

            let end_time = Duration::from_millis(next_s.timespan.end.abs().msecs() as u64);

            if current >= end_time {
                next_subtitle = subtitles.pop();
            }
        }

        i += 1;

        sleep_until(start + Duration::from_nanos(frame_times[i + 1] as u64)).await;
    }

    Ok(())
}
