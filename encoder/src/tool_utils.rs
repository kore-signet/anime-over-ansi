use std::fmt::Display;

#[cfg(feature = "cuda")]
use crate::cuda::CudaDitherer;
use colorful::pattern_dithering::MatrixSize;
use container::metadata::ColorMode;
use num_enum::TryFromPrimitive;

use crate::{ditherers, PreProcessor};

#[derive(TryFromPrimitive, Debug, PartialEq)]
#[repr(u8)]
pub enum SourceKind {
    Subtitles,
    Audio,
    Video,
}

impl SourceKind {
    pub fn from_parameters(params: ac_ffmpeg::codec::CodecParameters) -> Option<SourceKind> {
        if params.is_video_codec() {
            Some(SourceKind::Video)
        } else if params.is_audio_codec() {
            Some(SourceKind::Audio)
        } else if params.is_subtitle_codec() {
            Some(SourceKind::Subtitles)
        } else {
            None
        }
    }
}

pub struct SourceStreamMetadata {
    pub idx: usize,
    pub source_kind: SourceKind,
    pub codec_name: Option<&'static str>,
    pub title: Option<&'static str>,
}

impl Display for SourceStreamMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let SourceStreamMetadata {
            idx,
            codec_name,
            title,
            ..
        } = self;
        write!(
            f,
            "Stream {} ({}) \"{}\"",
            idx,
            codec_name.unwrap_or("unknown codec"),
            title
                .map(|v| v.to_string())
                .unwrap_or("untitled".to_string())
        )
    }
}

#[derive(Debug, Clone)]
pub enum AnsiTrack {
    SubtitleTrack(SubtitleTrack),
    VideoTrack(VideoTrack),
}

impl From<SubtitleTrack> for AnsiTrack {
    fn from(s: SubtitleTrack) -> Self {
        AnsiTrack::SubtitleTrack(s)
    }
}

impl From<VideoTrack> for AnsiTrack {
    fn from(s: VideoTrack) -> Self {
        AnsiTrack::VideoTrack(s)
    }
}

#[derive(Debug, Clone)]
pub struct SubtitleTrack {
    pub source_stream_index: usize,
    pub track_id: usize,
    pub track_name: String,
}

#[derive(Debug, Clone)]
pub struct VideoTrack {
    pub source_stream_index: usize,
    pub track_id: usize,
    pub track_name: String,
    pub track_height: usize,
    pub track_width: usize,
    pub color_mode: ColorMode,
    pub dither_mode: DitherConfig,
}

#[derive(Debug, Clone)]
pub struct DitherConfig {
    pub method: DitherMethod,
    pub distance_function: DistanceFunction,
    pub matrix_size: MatrixSize,
    pub multiplier: f32,
    pub width: u32,
    pub height: u32,
}

impl Default for DitherConfig {
    fn default() -> Self {
        Self {
            method: DitherMethod::FloydSteinberg,
            distance_function: DistanceFunction::CAM02,
            matrix_size: MatrixSize::Four,
            multiplier: 0.09,
            width: 192,
            height: 108,
        }
    }
}

impl DitherConfig {
    pub fn build(&self) -> Box<dyn PreProcessor<crate::video_encoder::DecodedVideoFrame> + Send> {
        match self.method {
            DitherMethod::FloydSteinberg => match self.distance_function {
                DistanceFunction::CAM02 => {
                    Box::new(ditherers::FloydSteinberg::<colorful::palette::CAM02>::new())
                }
                DistanceFunction::CIE94 => {
                    Box::new(ditherers::FloydSteinberg::<colorful::palette::CIE94>::new())
                }
                DistanceFunction::CIE76 => {
                    Box::new(ditherers::FloydSteinberg::<colorful::palette::CIE76>::new())
                }
            },
            DitherMethod::Pattern => {
                match self.distance_function {
                    DistanceFunction::CAM02 => Box::new(ditherers::Pattern::<
                        colorful::palette::CAM02,
                    >::new(
                        self.matrix_size, self.multiplier
                    )),
                    DistanceFunction::CIE94 => Box::new(ditherers::Pattern::<
                        colorful::palette::CIE94,
                    >::new(
                        self.matrix_size, self.multiplier
                    )),
                    DistanceFunction::CIE76 => Box::new(ditherers::Pattern::<
                        colorful::palette::CIE76,
                    >::new(
                        self.matrix_size, self.multiplier
                    )),
                }
            }
            #[cfg(feature = "cuda")]
            DitherMethod::Cuda => Box::new(
                CudaDitherer::new(self.width, self.height, self.multiplier, self.matrix_size)
                    .unwrap(),
            ),
            #[cfg(not(feature = "cuda"))]
            DitherMethod::Cuda => unreachable!()
        }
    }
}

#[derive(TryFromPrimitive, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum DistanceFunction {
    CAM02 = 0,
    CIE94 = 1,
    CIE76 = 2,
}

impl Display for DistanceFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DistanceFunction::CAM02 => write!(f, "CAM02"),
            DistanceFunction::CIE94 => write!(f, "CIE94"),
            DistanceFunction::CIE76 => write!(f, "CIE76"),
        }
    }
}

#[derive(TryFromPrimitive, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum DitherMethod {
    FloydSteinberg = 0,
    Pattern = 1,
    Cuda = 2,
}

impl Display for DitherMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            &Self::FloydSteinberg => write!(f, "floyd-steinberg"),
            &Self::Pattern => write!(f, "ordered pattern"),
            &Self::Cuda => write!(f, "cuda-accelerated ordered pattern"),
        }
    }
}

impl Default for SubtitleTrack {
    fn default() -> Self {
        Self {
            source_stream_index: usize::MAX,
            track_id: 0,
            track_name: "Subtitle 1".to_owned(),
        }
    }
}

impl Default for VideoTrack {
    fn default() -> Self {
        Self {
            source_stream_index: 0,
            track_id: 0,
            track_name: "Video 1".to_owned(),
            track_height: 108,
            track_width: 192,
            color_mode: ColorMode::True,
            dither_mode: DitherConfig::default(),
        }
    }
}

#[cfg(feature = "gui")]
use eframe::egui;

#[cfg(feature = "gui")]
impl AnsiTrack {
    pub fn show(&mut self, ui: &mut egui::Ui, source_streams: &[SourceStreamMetadata]) {
        match self {
            AnsiTrack::SubtitleTrack(s) => s.show(ui, source_streams),
            AnsiTrack::VideoTrack(s) => s.show(ui, source_streams),
        }
    }

    pub fn name(&self) -> String {
        match self {
            &AnsiTrack::SubtitleTrack(SubtitleTrack { ref track_name, .. }) => track_name.clone(),
            &AnsiTrack::VideoTrack(VideoTrack { ref track_name, .. }) => track_name.clone(),
        }
    }
}

#[cfg(feature = "gui")]
impl SubtitleTrack {
    pub fn show(&mut self, ui: &mut egui::Ui, source_streams: &[SourceStreamMetadata]) {
        egui::Grid::new(self.track_id)
            .num_columns(2)
            .spacing([60.0, 7.0])
            .striped(false)
            .show(ui, |ui| {
                self.track_editor(ui, source_streams);
            });
    }

    pub fn track_editor(&mut self, ui: &mut egui::Ui, source_streams: &[SourceStreamMetadata]) {
        ui.label("Source");
        egui::ComboBox::from_id_source("Source")
            .selected_text(format!(
                "{}",
                source_streams
                    .iter()
                    .find(|t| t.idx == self.source_stream_index)
                    .map(|v| v as &dyn Display)
                    .unwrap_or(&"unknown" as &dyn Display)
            ))
            .show_ui(ui, |ui| {
                for source in source_streams
                    .iter()
                    .filter(|v| v.source_kind == SourceKind::Subtitles)
                {
                    ui.selectable_value(
                        &mut self.source_stream_index,
                        source.idx,
                        format!("{}", source),
                    );
                }
            });
        ui.end_row();

        ui.label("Track name");
        ui.text_edit_singleline(&mut self.track_name);
        ui.end_row();
    }
}

#[cfg(feature = "gui")]
impl VideoTrack {
    pub fn show(&mut self, ui: &mut egui::Ui, source_streams: &[SourceStreamMetadata]) {
        egui::Grid::new(self.track_id)
            .num_columns(2)
            .spacing([60.0, 7.0])
            .striped(false)
            .show(ui, |ui| {
                self.track_editor(ui, source_streams);
            });
    }

    pub fn track_editor(&mut self, ui: &mut egui::Ui, source_streams: &[SourceStreamMetadata]) {
        ui.label("Source");
        egui::ComboBox::from_id_source("Source")
            .selected_text(format!(
                "{}",
                source_streams
                    .iter()
                    .find(|t| t.idx == self.source_stream_index)
                    .map(|v| v as &dyn Display)
                    .unwrap_or(&"unknown" as &dyn Display)
            ))
            .show_ui(ui, |ui| {
                for source in source_streams
                    .iter()
                    .filter(|v| v.source_kind == SourceKind::Video)
                {
                    ui.selectable_value(
                        &mut self.source_stream_index,
                        source.idx,
                        format!("{}", source),
                    );
                }
            });
        ui.end_row();

        ui.label("Track name");
        ui.text_edit_singleline(&mut self.track_name);
        ui.end_row();

        ui.label("Dimensions");
        ui.horizontal(|ui| {
            ui.label("Width");
            ui.add(egui::DragValue::new(&mut self.track_width));
            ui.add_space(5.0);
            ui.label("Height");
            ui.add(egui::DragValue::new(&mut self.track_height));
        });

        ui.end_row();

        ui.label("Color mode");
        egui::ComboBox::from_id_source("Color mode")
            .selected_text(format!("{}", self.color_mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.color_mode, ColorMode::True, "true color");
                ui.selectable_value(&mut self.color_mode, ColorMode::EightBit, "eight-bit");
            });
        ui.end_row();

        if self.color_mode == ColorMode::EightBit {
            ui.collapsing("Dithering settings", |ui| {
                egui::Grid::new("dither_grid")
                    .num_columns(2)
                    .spacing([40.0, 7.0])
                    .striped(false)
                    .show(ui, |ui| self.dither_settings(ui));
            });
        }

        ui.end_row();
    }

    pub fn dither_settings(&mut self, ui: &mut egui::Ui) {
        ui.label("Method");
        egui::ComboBox::from_id_source("Dither mode")
            .selected_text(format!("{}", self.dither_mode.method))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.dither_mode.method,
                    DitherMethod::FloydSteinberg,
                    "floyd-steinberg",
                );
                ui.selectable_value(
                    &mut self.dither_mode.method,
                    DitherMethod::Pattern,
                    "ordered pattern",
                );
            });
        ui.end_row();

        ui.label("Distance function");
        egui::ComboBox::from_id_source("Distance function")
            .selected_text(format!("{}", self.dither_mode.distance_function))
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.dither_mode.distance_function,
                    DistanceFunction::CAM02,
                    "CAM02 (best)",
                );
                ui.selectable_value(
                    &mut self.dither_mode.distance_function,
                    DistanceFunction::CIE94,
                    "CIE94",
                );
                ui.selectable_value(
                    &mut self.dither_mode.distance_function,
                    DistanceFunction::CIE76,
                    "CIE76 (fastest)",
                );
            });
        ui.end_row();

        if self.dither_mode.method == DitherMethod::Pattern {
            ui.label("Matrix size");
            egui::ComboBox::from_id_source("matrix size")
                .selected_text(format!("{}", self.dither_mode.matrix_size))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.dither_mode.matrix_size,
                        MatrixSize::Eight,
                        "8x8 (best, slow)",
                    );
                    ui.selectable_value(&mut self.dither_mode.matrix_size, MatrixSize::Four, "4x4");
                    ui.selectable_value(
                        &mut self.dither_mode.matrix_size,
                        MatrixSize::Two,
                        "2x2 (fastest)",
                    );
                });
            ui.end_row();

            ui.label("Error multiplier");
            ui.add(egui::DragValue::new(&mut self.dither_mode.multiplier).speed(0.01));
            ui.end_row();
        }
    }
}
