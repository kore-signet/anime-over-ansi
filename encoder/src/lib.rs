use anime_telnet::{encoding::*, metadata::*};

use opencv::core::{Mat, MatTraitConst, Vector};
use opencv::videoio::{VideoCapture, VideoCaptureTrait};

use crossbeam::channel::bounded;
use image::{RgbImage, RgbaImage};
use indicatif::{ProgressBar, ProgressStyle};

pub fn encode(
    video_cap: &mut VideoCapture,
    pipelines: &Vec<ProcessorPipeline>,
    tracks: &mut Vec<(Encoder, VideoTrack)>,
    frame_quant: u64,
    width: u32,
    height: u32,
    show_progress: bool,
) -> std::io::Result<()> {
    let mut mat = Mat::default();
    let mut rgb_mat = Mat::default();

    let encoder_bar = ProgressBar::new(frame_quant as u64);
    if !show_progress {
        encoder_bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    } else {
        encoder_bar.set_style(
            ProgressStyle::default_bar()
                .template("{per_sec:5!}fps, ETA {eta} - {percent}% done, encoding frame {pos} out of {len}\n{bar:40.green/white}"),
        );
    }

    let mut buffer = Vector::<u8>::with_capacity((width * height * 3) as usize);

    let (snd_img, rcv_img) = bounded(64);
    let (snd_resized, rcv_resized) = bounded(64);

    crossbeam::scope(|s| {
        s.spawn(|_| {
            while video_cap.read(&mut mat).unwrap() {
                opencv::imgproc::cvt_color(
                    &mat,
                    &mut rgb_mat,
                    opencv::imgproc::ColorConversionCodes::COLOR_BGR2RGBA as i32,
                    0,
                )
                .unwrap();

                rgb_mat.reshape(1, 1).unwrap().copy_to(&mut buffer).unwrap();
                snd_img
                    .send(
                        RgbaImage::from_raw(width as u32, height as u32, buffer.to_vec()).unwrap(),
                    )
                    .unwrap();
            }

            drop(snd_img);
        });

        s.spawn(|_| {
            for img in rcv_img.iter() {
                snd_resized
                    .send(
                        pipelines
                            .iter()
                            .flat_map(|p| {
                                p.process(&img)
                                    .into_iter()
                                    .map(move |r| (p.width, p.height, r.0, r.1))
                            })
                            .collect::<Vec<(u32, u32, ColorMode, RgbImage)>>(),
                    )
                    .unwrap();
            }

            drop(snd_resized);
        });

        for msg in rcv_resized.iter() {
            for (encoder, _) in tracks.iter_mut() {
                encoder
                    .encode_frame(
                        &msg[msg
                            .iter()
                            .position(|v| {
                                v.0 == encoder.needs_width
                                    && v.1 == encoder.needs_height
                                    && v.2 == encoder.needs_color
                            })
                            .unwrap()]
                        .3,
                    )
                    .unwrap();
            }

            encoder_bar.inc(1);
        }

        encoder_bar.finish();
    })
    .unwrap();

    Ok(())
}
