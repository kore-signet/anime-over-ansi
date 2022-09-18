use std::path::PathBuf;
use std::sync::Arc;

use bytes::BytesMut;
use container::metadata::{ColorMode, CompressionMode, SubtitleFormat};
use container::packet::*;
use eframe::{
    egui::{self, CollapsingHeader, RichText},
    epaint::Color32,
};
use encoder::tool_utils::*;
use encoder::video_encoder::*;
use encoder::*;
use postage::prelude::*;
use tokio::{fs::File, io::BufWriter};

fn main() {
    let mut options = eframe::NativeOptions::default();

    options.resizable = true;
    // let mut source = FFMpegSource::open_url("nichijou.mp4").unwrap();
    // for stream in source.streams() {
    //     println!("{:?}", stream.codec_parameters().decoder_name());
    // }

    eframe::run_native(
        "ansi.moe",
        options,
        Box::new(|cc| {
            let mut style = (*cc.egui_ctx.style()).clone();

            for (_, fontid) in style.text_styles.iter_mut() {
                fontid.size *= 1.5;
            }

            cc.egui_ctx.set_style(style);

            Box::new(App::default())
        }),
    );
}

#[derive(Default)]
struct App {
    ff_source: Option<FFMpegSource>,
    picked_path: Option<RichText>,
    ansi_tracks: Vec<AnsiTrack>,
    deleting: Option<usize>,        // popup to confirm delete is open
    imminent_delete: Option<usize>, // will be deleted next update
    source_streams: Vec<SourceStreamMetadata>,
    render_rx: Option<tokio::sync::watch::Receiver<(f64, u64)>>,
    tokio_rt: Option<tokio::runtime::Runtime>,
}

impl App {
    fn open_new_file(&mut self, path: PathBuf) {
        match FFMpegSource::open_url(&path.display().to_string()) {
            Ok(source) => {
                self.picked_path = Some(RichText::new(path.display().to_string()).monospace());

                self.source_streams.clear();

                for (i, stream) in source.streams().iter().enumerate() {
                    if let Some(kind) = SourceKind::from_parameters(stream.codec_parameters()) {
                        let meta = SourceStreamMetadata {
                            idx: i,
                            source_kind: kind,
                            codec_name: stream.codec_parameters().decoder_name(),
                            title: stream.get_metadata("title"),
                        };
                        self.source_streams.push(meta);
                    }
                }

                self.ff_source = Some(source);
            }
            Err(e) => {
                self.picked_path = Some(RichText::new(e.to_string()).color(Color32::RED));
            }
        }
    }

    fn show_main_panel(&mut self, ui: &mut egui::Ui) {
        if ui.button("📁 open video file").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                self.open_new_file(path);
            }
        }

        if let Some(path) = &self.picked_path {
            ui.label(path.clone());
        }

        ui.end_row();

        if self.ff_source.is_none() {
            return;
        }

        if let Some(deletion_idx) = self.imminent_delete.take() {
            self.deleting = None;
            if deletion_idx < self.ansi_tracks.len() {
                self.ansi_tracks.remove(deletion_idx);
            }
        }

        if ui
            .button(
                RichText::new("➕ Add Video Track")
                    .strong()
                    .color(Color32::BLUE),
            )
            .clicked()
        {
            let mut track = VideoTrack::default();
            track.track_id = self.ansi_tracks.len() + 1;
            track.track_name = format!("Video {}", track.track_id);
            self.ansi_tracks.push(track.into());
        }

        ui.end_row();

        if ui
            .button(
                RichText::new("➕ Add Subtitle Track")
                    .strong()
                    .color(Color32::DARK_GREEN),
            )
            .clicked()
        {
            let mut track = SubtitleTrack::default();
            track.track_id = self.ansi_tracks.len() + 1;
            track.track_name = format!("Subtitles {}", track.track_id);
            self.ansi_tracks.push(track.into());
        }

        ui.end_row();

        for (i, track) in self.ansi_tracks.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                // TODO: set collapsingheader id to numerical index id
                CollapsingHeader::new(track.name())
                    .id_source(i)
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            let response = ui
                                .button(RichText::new("delete track").strong().color(Color32::RED));
                            let popup_id = ui.make_persistent_id(format!("delete-{}", i));

                            if response.clicked() {
                                self.deleting = Some(i);
                            }

                            match self.deleting {
                                Some(idx) if idx == i => ui.memory().open_popup(popup_id),
                                _ => (),
                            }

                            egui::popup::popup_below_widget(ui, popup_id, &response, |ui| {
                                ui.set_min_width(250.0);
                                ui.label("Are you sure you want to delete this track?");
                                ui.horizontal(|ui| {
                                    if ui
                                        .button(RichText::new("🗙 Yes").strong().color(Color32::RED))
                                        .clicked()
                                    {
                                        self.imminent_delete = Some(i);
                                        return;
                                    }

                                    if ui
                                        .button(
                                            RichText::new("↺ No")
                                                .strong()
                                                .color(Color32::DARK_GREEN),
                                        )
                                        .clicked()
                                    {
                                        self.deleting = None;
                                        return;
                                    }
                                });
                            });

                            ui.add_space(10.0);
                            track.show(ui, &self.source_streams);
                        });
                    });
            });
            ui.end_row();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::bottom("my_panel")
            .min_height(50.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if ui.button("💾 render").clicked() {
                        if let Some(path) = rfd::FileDialog::new().save_file() {
                            let (state_tx, state_rx) = tokio::sync::watch::channel((0.0, 1));
                            let rt = tokio::runtime::Runtime::new().unwrap();

                            let mut video_tracks = Vec::new();
                            let mut subtitle_tracks = Vec::new();

                            let (encoded_packet_tx, encoded_packet_rx) = tokio::sync::mpsc::channel::<container::packet::Packet<BytesMut>>(120);
                            let (source_packet_pipe, source_packet_receiver) =
                                postage::broadcast::channel::<Arc<FFMpegPacket>>(255);

                            let mut pipes = Vec::with_capacity(self.ansi_tracks.len());

                            println!("{:#?}", self.ansi_tracks);

                            for track in self.ansi_tracks.iter().cloned() {
                                match track {
                                    AnsiTrack::VideoTrack(t) => {
                                        let decoder = FFMpegVideoDecoder::from_stream(
                                            &self.ff_source.as_ref().unwrap().streams()[t.source_stream_index],
                                            ac_ffmpeg::codec::video::scaler::Algorithm::Lanczos,
                                            t.track_width,
                                            t.track_height,
                                        ).unwrap();

                                        let encoder = FrameEncoder {
                                            stream_index: t.track_id as u16,
                                            width: t.track_width as u32,
                                            height: t.track_height as u32,
                                            color: t.color_mode,
                                            use_diffing: false,
                                            last_frame: None,
                                        };

                                        match t.color_mode {
                                            ColorMode::True => {
                                                pipes.push(pipeline! {
                                                    receive from source_packet_receiver;
                                                    send to encoded_packet_tx;
                                                    stream t.source_stream_index => decoder => passthrough => encoder
                                                });
                                            }
                                            ColorMode::EightBit => {
                                                pipes.push(pipeline! {
                                                    receive from source_packet_receiver;
                                                    send to encoded_packet_tx;
                                                    stream t.source_stream_index => decoder => t.dither_mode.build() => encoder
                                                });
                                            },
                                        }
                                        video_tracks.push(container::metadata::VideoTrack {
                                            name: Some(t.track_name.clone()),
                                            color_mode: t.color_mode,
                                            height: t.track_height as u32,
                                            width: t.track_width as u32,
                                            codec_private: None,
                                            index: t.track_id as u16,
                                        })
                                    }
                                    AnsiTrack::SubtitleTrack(t) => {
                                       pipes.push(pipeline! {
                                            receive from source_packet_receiver;
                                            send to encoded_packet_tx;
                                            stream t.source_stream_index => GenericPacketDecoder::override_stream_index(t.track_id as u16) => passthrough => passthrough
                                        });

                                        let parameters = self.ff_source.as_ref().unwrap().streams()
                                            [t.source_stream_index]
                                            .codec_parameters();

                                        subtitle_tracks.push(container::metadata::SubtitleTrack {
                                            name: Some(t.track_name.clone()),
                                            lang: None,
                                            format: SubtitleFormat::Unknown(
                                                parameters
                                                    .encoder_name()
                                                    .map(|v| v.to_owned())
                                                    .unwrap_or("unknown".to_owned()),
                                            ),
                                            codec_private: parameters
                                                .extradata()
                                                .map(|v| v.to_vec()),
                                            index: t.track_id as u16,
                                        })
                                    }
                                }
                            }

                            let video_metadata = container::metadata::VideoMetadata {
                                video_tracks,
                                subtitle_tracks,
                                attachments: Vec::new(),
                                compression: CompressionMode::None
                            };

                            let router = route_source(source_packet_pipe, self.ff_source.take().unwrap(), pipes);

                            let mut waker_rx = state_rx.clone();
                            let waker_ctx = ctx.clone();

                            rt.spawn(async move {
                                let output_file = BufWriter::new(File::create(path).await.unwrap());
                                let writer = write_with_container_metadata(video_metadata,
                                    output_file,
                                    encoded_packet_rx,
                                    state_tx,
                                ());
tokio::task::spawn(async move {
                                    while waker_rx.changed().await.is_ok() {
                                        waker_ctx.request_repaint();
                                    }
                                });

                                tokio::join!(router, writer);
                            });

                            self.render_rx = Some(state_rx);
                            self.tokio_rt = Some(rt);
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(rx) = self.render_rx.as_ref() {
                let (fps, total) = *rx.borrow();
                if fps.is_nan() {
                    ui.label("finished");
                } else {
                    ui.label(format!("total {total} - {fps:.1} fps"));
                }
            } else {
                egui::Grid::new("main_grid")
                    .num_columns(1)
                    .striped(false)
                    .spacing([20.0, 8.0])
                    .show(ui, |ui| self.show_main_panel(ui));
            }
        });
    }
}
