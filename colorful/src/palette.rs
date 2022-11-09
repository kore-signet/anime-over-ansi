use super::delta;
use arrayvec::ArrayString;
use kasi_kule::{Jab, UCS};
use lab::Lab;
use lazy_static::lazy_static;
use std::{collections::HashMap, marker::PhantomData};

// TODO: configurable palettes

pub const PALETTE: [[u8; 3]; 256] = [
    // idx: ansi colorid. tuple is r,g,b
    [0, 0, 0],
    [128, 0, 0],
    [0, 128, 0],
    [128, 128, 0],
    [0, 0, 128],
    [128, 0, 128],
    [0, 128, 128],
    [192, 192, 192],
    [128, 128, 128],
    [255, 0, 0],
    [0, 255, 0],
    [255, 255, 0],
    [0, 0, 255],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
    [0, 0, 0],
    [0, 0, 95],
    [0, 0, 135],
    [0, 0, 175],
    [0, 0, 215],
    [0, 0, 255],
    [0, 95, 0],
    [0, 95, 95],
    [0, 95, 135],
    [0, 95, 175],
    [0, 95, 215],
    [0, 95, 255],
    [0, 135, 0],
    [0, 135, 95],
    [0, 135, 135],
    [0, 135, 175],
    [0, 135, 215],
    [0, 135, 255],
    [0, 175, 0],
    [0, 175, 95],
    [0, 175, 135],
    [0, 175, 175],
    [0, 175, 215],
    [0, 175, 255],
    [0, 215, 0],
    [0, 215, 95],
    [0, 215, 135],
    [0, 215, 175],
    [0, 215, 215],
    [0, 215, 255],
    [0, 255, 0],
    [0, 255, 95],
    [0, 255, 135],
    [0, 255, 175],
    [0, 255, 215],
    [0, 255, 255],
    [95, 0, 0],
    [95, 0, 95],
    [95, 0, 135],
    [95, 0, 175],
    [95, 0, 215],
    [95, 0, 255],
    [95, 95, 0],
    [95, 95, 95],
    [95, 95, 135],
    [95, 95, 175],
    [95, 95, 215],
    [95, 95, 255],
    [95, 135, 0],
    [95, 135, 95],
    [95, 135, 135],
    [95, 135, 175],
    [95, 135, 215],
    [95, 135, 255],
    [95, 175, 0],
    [95, 175, 95],
    [95, 175, 135],
    [95, 175, 175],
    [95, 175, 215],
    [95, 175, 255],
    [95, 215, 0],
    [95, 215, 95],
    [95, 215, 135],
    [95, 215, 175],
    [95, 215, 215],
    [95, 215, 255],
    [95, 255, 0],
    [95, 255, 95],
    [95, 255, 135],
    [95, 255, 175],
    [95, 255, 215],
    [95, 255, 255],
    [135, 0, 0],
    [135, 0, 95],
    [135, 0, 135],
    [135, 0, 175],
    [135, 0, 215],
    [135, 0, 255],
    [135, 95, 0],
    [135, 95, 95],
    [135, 95, 135],
    [135, 95, 175],
    [135, 95, 215],
    [135, 95, 255],
    [135, 135, 0],
    [135, 135, 95],
    [135, 135, 135],
    [135, 135, 175],
    [135, 135, 215],
    [135, 135, 255],
    [135, 175, 0],
    [135, 175, 95],
    [135, 175, 135],
    [135, 175, 175],
    [135, 175, 215],
    [135, 175, 255],
    [135, 215, 0],
    [135, 215, 95],
    [135, 215, 135],
    [135, 215, 175],
    [135, 215, 215],
    [135, 215, 255],
    [135, 255, 0],
    [135, 255, 95],
    [135, 255, 135],
    [135, 255, 175],
    [135, 255, 215],
    [135, 255, 255],
    [175, 0, 0],
    [175, 0, 95],
    [175, 0, 135],
    [175, 0, 175],
    [175, 0, 215],
    [175, 0, 255],
    [175, 95, 0],
    [175, 95, 95],
    [175, 95, 135],
    [175, 95, 175],
    [175, 95, 215],
    [175, 95, 255],
    [175, 135, 0],
    [175, 135, 95],
    [175, 135, 135],
    [175, 135, 175],
    [175, 135, 215],
    [175, 135, 255],
    [175, 175, 0],
    [175, 175, 95],
    [175, 175, 135],
    [175, 175, 175],
    [175, 175, 215],
    [175, 175, 255],
    [175, 215, 0],
    [175, 215, 95],
    [175, 215, 135],
    [175, 215, 175],
    [175, 215, 215],
    [175, 215, 255],
    [175, 255, 0],
    [175, 255, 95],
    [175, 255, 135],
    [175, 255, 175],
    [175, 255, 215],
    [175, 255, 255],
    [215, 0, 0],
    [215, 0, 95],
    [215, 0, 135],
    [215, 0, 175],
    [215, 0, 215],
    [215, 0, 255],
    [215, 95, 0],
    [215, 95, 95],
    [215, 95, 135],
    [215, 95, 175],
    [215, 95, 215],
    [215, 95, 255],
    [215, 135, 0],
    [215, 135, 95],
    [215, 135, 135],
    [215, 135, 175],
    [215, 135, 215],
    [215, 135, 255],
    [215, 175, 0],
    [215, 175, 95],
    [215, 175, 135],
    [215, 175, 175],
    [215, 175, 215],
    [215, 175, 255],
    [215, 215, 0],
    [215, 215, 95],
    [215, 215, 135],
    [215, 215, 175],
    [215, 215, 215],
    [215, 215, 255],
    [215, 255, 0],
    [215, 255, 95],
    [215, 255, 135],
    [215, 255, 175],
    [215, 255, 215],
    [215, 255, 255],
    [255, 0, 0],
    [255, 0, 95],
    [255, 0, 135],
    [255, 0, 175],
    [255, 0, 215],
    [255, 0, 255],
    [255, 95, 0],
    [255, 95, 95],
    [255, 95, 135],
    [255, 95, 175],
    [255, 95, 215],
    [255, 95, 255],
    [255, 135, 0],
    [255, 135, 95],
    [255, 135, 135],
    [255, 135, 175],
    [255, 135, 215],
    [255, 135, 255],
    [255, 175, 0],
    [255, 175, 95],
    [255, 175, 135],
    [255, 175, 175],
    [255, 175, 215],
    [255, 175, 255],
    [255, 215, 0],
    [255, 215, 95],
    [255, 215, 135],
    [255, 215, 175],
    [255, 215, 215],
    [255, 215, 255],
    [255, 255, 0],
    [255, 255, 95],
    [255, 255, 135],
    [255, 255, 175],
    [255, 255, 215],
    [255, 255, 255],
    [8, 8, 8],
    [18, 18, 18],
    [28, 28, 28],
    [38, 38, 38],
    [48, 48, 48],
    [58, 58, 58],
    [68, 68, 68],
    [78, 78, 78],
    [88, 88, 88],
    [98, 98, 98],
    [108, 108, 108],
    [118, 118, 118],
    [128, 128, 128],
    [138, 138, 138],
    [148, 148, 148],
    [158, 158, 158],
    [168, 168, 168],
    [178, 178, 178],
    [188, 188, 188],
    [198, 198, 198],
    [208, 208, 208],
    [218, 218, 218],
    [228, 228, 228],
    [238, 238, 238],
];

lazy_static! {
    pub static ref REVERSE_PALETTE: HashMap<[u8; 3], u8> = {
        let mut pal = HashMap::new();
        for (i, c) in PALETTE.iter().enumerate() {
            pal.insert(*c, i as u8);
        }
        pal
    };

    /// Static ansi256 palette in LAB format, converted from RGB.
    pub static ref LAB_PALETTE: [(f32, f32, f32); 256] = {
        let mut pal = [(0.0f32,0.0f32,0.0f32); 256];
        for i in 0..256 {
            let lab = Lab::from_rgb(&PALETTE[i]);
            pal[i] = (lab.l, lab.a, lab.b)
        }

        pal
    };

    /// Static ansi256 palette in LAB format, converted from RGB. Flattened from tuple representation and with an extra zero added for easier handling with SIMD intrinsics.
    pub static ref LAB_PALETTE_FLATTENED: [f32; 1024] = {
        let mut pal = [0.0f32; 1024];
        for i in 0..256 {
            let lab = Lab::from_rgb(&PALETTE[i]);
            pal[i * 4] = lab.l;
            pal[i * 4 + 1] = lab.a;
            pal[i * 4 + 2] = lab.b;
            pal[i * 4 + 3] = 0.0;
        }

        pal
    };

    pub static ref JAB_PALETTE: [(f32, f32, f32); 256] = {
        let mut pal = [(0.0f32,0.0f32,0.0f32); 256];
        for i in 0..256 {
            let jab = Jab::<UCS>::from(PALETTE[i]);
            pal[i] = (jab.J, jab.a, jab.b)
        }

        pal
    };

    pub static ref JAB_PALETTE_FLATTENED: [f32; 1024] = {
        let mut pal = [0.0f32; 1024];
        for i in 0..256 {
            let jab = Jab::<UCS>::from(PALETTE[i]);
            pal[i * 4] = jab.J;
            pal[i * 4 + 1] = jab.a;
            pal[i * 4 + 2] = jab.b;
            pal[i * 4 + 3] = 0.0;
        }

        pal
    };

    pub static ref PALETTE_FG_CODES: [ArrayString<20>; 256] = {
        let mut out = [ArrayString::new_const(); 256];

        for (i, _) in PALETTE.iter().enumerate() {
            out[i] = ArrayString::from(&format!("\x1B[38;5;{}m", i)).unwrap();
        }


        out
    };

    pub static ref PALETTE_BG_CODES: [ArrayString<20>; 256] = {
        let mut out = [ArrayString::new_const(); 256];

        for (i, _) in PALETTE.iter().enumerate() {
            out[i] = ArrayString::from(&format!("\x1B[48;5;{}m", i)).unwrap();
        }


        out
    };

    pub static ref REVERSE_PALETTE_FG_CODES: HashMap<[u8; 3], ArrayString<20>> = {
        let mut pal = HashMap::new();
        for (i, c) in PALETTE.iter().enumerate() {
            pal.insert(*c, ArrayString::from(&format!("\x1B[38;5;{}m", i)).unwrap());
        }

        pal
    };

    pub static ref REVERSE_PALETTE_BG_CODES: HashMap<[u8; 3], ArrayString<20>> = {
        let mut pal = HashMap::new();
        for (i, c) in PALETTE.iter().enumerate() {
            pal.insert(*c, ArrayString::from(&format!("\x1B[48;5;{}m", i)).unwrap());
        }

        pal
    };
}

use image::{imageops::ColorMap, Rgb};

pub trait DistanceMethod {
    fn closest(color: &[u8; 3]) -> usize;
}

macro_rules! distance_method {
    ($name:ident : $func:path) => {
        #[derive(Copy, Clone, Debug)]
        pub struct $name;

        impl DistanceMethod for $name {
            #[inline(always)]
            fn closest(color: &[u8; 3]) -> usize {
                $func(color).0 as usize
            }
        }
    };
}

distance_method!(CAM02: delta::jab::closest_ansi);
distance_method!(CIE94: delta::cie94::closest_ansi);
distance_method!(CIE76: delta::cie76::closest_ansi);

#[derive(Clone, Copy, Debug)]
pub struct AnsiColorMap<T: DistanceMethod> {
    _spooky: PhantomData<T>,
}

impl<T: DistanceMethod> AnsiColorMap<T> {
    pub fn new() -> AnsiColorMap<T> {
        AnsiColorMap {
            _spooky: PhantomData,
        }
    }
}

impl<T: DistanceMethod> ColorMap for AnsiColorMap<T> {
    type Color = Rgb<u8>;

    #[inline(always)]
    fn index_of(&self, color: &Rgb<u8>) -> usize {
        T::closest(&color.0)
    }

    #[inline(always)]
    fn lookup(&self, idx: usize) -> Option<Self::Color> {
        Some(Rgb(PALETTE[idx]))
    }

    #[inline(always)]
    fn has_lookup(&self) -> bool {
        true
    }

    #[inline(always)]
    fn map_color(&self, color: &mut Rgb<u8>) {
        *color = self.lookup(self.index_of(color)).unwrap();
    }
}
