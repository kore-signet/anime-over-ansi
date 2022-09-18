#![allow(unused_must_use)]

use bytes::{BufMut, BytesMut};
use colorful::palette::*;
use container::metadata::*;
use image::{Rgb, RgbImage};
use std::fmt::Write;

// A base trait for any ANSI image frame encoder, automatically implementing most of the encoding based on a few getter methods.
pub trait AnsiEncoder {
    #[inline(always)]
    fn color(&self, pixel: &Rgb<u8>, fg: bool, out: &mut impl BufMut) {
        match self.needs_color() {
            ColorMode::EightBit => out.put_slice(if fg {
                REVERSE_PALETTE_FG_CODES[&pixel.0].as_bytes()
            } else {
                REVERSE_PALETTE_BG_CODES[&pixel.0].as_bytes()
            }),
            _ => {
                if fg {
                    out.put_slice(b"\x1b[38;2;");
                } else {
                    out.put_slice(b"\x1b[48;2;");
                }

                let mut buffer = itoa::Buffer::new();
                out.put_slice(buffer.format(pixel[0]).as_bytes());
                out.put_u8(b';');
                out.put_slice(buffer.format(pixel[1]).as_bytes());
                out.put_u8(b';');
                out.put_slice(buffer.format(pixel[2]).as_bytes());
                out.put_u8(b'm');
            }
        }
    }

    fn encode_frame(&mut self, image: &RgbImage) -> BytesMut {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut frame = BytesMut::with_capacity((image.width() * image.height() * 20) as usize);
        for y in (0..image.height() - 1).step_by(2) {
            for x in 0..image.width() {
                let upper = image.get_pixel(x, y);
                let lower = image.get_pixel(x, y + 1);

                if last_upper.is_none() || &last_upper.unwrap() != upper {
                    self.color(upper, true, &mut frame);
                }

                if last_lower.is_none() || &last_lower.unwrap() != lower {
                    self.color(lower, false, &mut frame);
                }

                frame.put_slice(b"\xE2\x96\x80");

                last_upper = Some(*upper);
                last_lower = Some(*lower);
            }

            frame.put_slice(b"\x1b[1E");
        }

        frame
    }

    fn encode_diffed_frame(&self, image: &RgbImage, old_img: &RgbImage) -> BytesMut {
        let mut last_upper: Option<Rgb<u8>> = None;
        let mut last_lower: Option<Rgb<u8>> = None;

        let mut last_x = 0;

        let mut frame = BytesMut::with_capacity((image.width() * image.height()) as usize);
        for y in (0..image.height() - 1).step_by(2) {
            for x in 0..image.width() {
                let upper = image.get_pixel(x, y);
                let lower = image.get_pixel(x, y + 1);

                if old_img.get_pixel(x, y) != upper || old_img.get_pixel(x, y + 1) != lower {
                    if last_x != x + 1 {
                        write!(frame, "\x1b[{}G", x + 1);
                    }

                    if last_upper.is_none() || &last_upper.unwrap() != upper {
                        self.color(upper, true, &mut frame);
                    }

                    if last_lower.is_none() || &last_lower.unwrap() != lower {
                        self.color(lower, false, &mut frame);
                    }

                    frame.put_slice(b"\xE2\x96\x80"); // "▀"

                    last_upper = Some(*upper);
                    last_lower = Some(*lower);

                    last_x = x;
                }
            }

            frame.put_slice(b"\x1b[1E");
            last_x = 0;
        }

        frame
    }

    fn encode_best(&mut self, image: &RgbImage) -> (BytesMut, bool) {
        let use_diffing = self.use_diffing();

        if use_diffing {
            if let Some(last_frame) = self.replace_last_frame(image.clone()) {
                let non_diffed = self.encode_frame(image);
                let diffed = self.encode_diffed_frame(image, &last_frame);

                if diffed.len() > non_diffed.len() {
                    return (non_diffed, true);
                } else {
                    return (diffed, false);
                }
            }
        }

        (self.encode_frame(image), true)
    }

    fn needs_width(&self) -> u32;
    fn needs_height(&self) -> u32;
    fn needs_color(&self) -> ColorMode;

    fn use_diffing(&self) -> bool {
        false
    }

    fn replace_last_frame(&mut self, new_frame: RgbImage) -> Option<RgbImage>;
}
