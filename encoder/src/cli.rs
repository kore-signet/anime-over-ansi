use colorful::pattern_dithering::MatrixSize;
use container::metadata::ColorMode;

use crate::tool_utils::{
    AnsiTrack, DistanceFunction, DitherMethod, SourceStreamMetadata, SubtitleTrack, VideoTrack,
};

pub fn select_video_track(
    video_sources: &[SourceStreamMetadata],
    track_id: usize,
) -> anyhow::Result<AnsiTrack> {
    let theme = dialoguer::theme::ColorfulTheme::default();
    let mut track = VideoTrack::default();

    let source = &video_sources[dialoguer::Select::with_theme(&theme)
        .with_prompt("choose source stream")
        .items(&video_sources)
        .interact()?];

    track.source_stream_index = source.idx;
    track.track_id = track_id;

    track.track_name = dialoguer::Input::with_theme(&theme)
        .with_prompt("track name")
        .default(
            source
                .title
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("Video {}", track.track_id)),
        )
        .interact_text()?;

    track.track_width = dialoguer::Input::with_theme(&theme)
        .with_prompt("video width")
        .default("192".to_string())
        .validate_with(|input: &String| input.parse::<usize>().map(|_| ()))
        .interact_text()?
        .parse::<usize>()?;

    track.track_height = dialoguer::Input::with_theme(&theme)
        .with_prompt("video height")
        .default("108".to_string())
        .validate_with(|input: &String| input.parse::<usize>().map(|_| ()))
        .interact_text()?
        .parse::<usize>()?;

    track.color_mode = ColorMode::try_from(
        dialoguer::Select::with_theme(&theme)
            .with_prompt("color mode")
            .item("true color")
            .item("256color/8bit")
            .interact()? as u8,
    )?;

    if track.color_mode == ColorMode::True {
        return Ok(AnsiTrack::VideoTrack(track));
    }

    track.dither_mode.method = DitherMethod::try_from(
        dialoguer::Select::with_theme(&theme)
            .with_prompt("dither method")
            .item("floyd-steinberg")
            .item("ordered pattern dithering")
            .interact()? as u8,
    )?;

    track.dither_mode.distance_function = DistanceFunction::try_from(
        dialoguer::Select::with_theme(&theme)
            .with_prompt("color distance function")
            .item("CAM02 (best, fast)")
            .item("CIE94 (medium, slowest)")
            .item("CIE76 (worst, fastest)")
            .interact()? as u8,
    )?;

    if track.dither_mode.method == DitherMethod::FloydSteinberg {
        return Ok(AnsiTrack::VideoTrack(track));
    }

    track.dither_mode.matrix_size = MatrixSize::try_from(
        dialoguer::Select::with_theme(&theme)
            .with_prompt("matrix size")
            .item("8x8 (best, slowest)")
            .item("4x4")
            .item("2x2 (worst, fastest)")
            .interact()? as u8,
    )?;

    track.dither_mode.multiplier = dialoguer::Input::with_theme(&theme)
        .with_prompt("color difference multiplier")
        .default("0.09".to_string())
        .validate_with(|v: &String| v.parse::<f32>().map(|_| ()))
        .interact_text()?
        .parse::<f32>()?;

    Ok(AnsiTrack::VideoTrack(track))
}

pub fn select_subtitle_track(
    subtitle_sources: &[SourceStreamMetadata],
    track_id: usize,
) -> anyhow::Result<AnsiTrack> {
    let theme = dialoguer::theme::ColorfulTheme::default();
    let mut track = SubtitleTrack::default();

    let source = &subtitle_sources[dialoguer::Select::with_theme(&theme)
        .with_prompt("choose source stream")
        .items(&subtitle_sources)
        .interact()?];

    track.source_stream_index = source.idx;
    track.track_id = track_id;

    track.track_name = dialoguer::Input::with_theme(&theme)
        .with_prompt("track name")
        .default(
            source
                .title
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("Subtitles {}", track.track_id)),
        )
        .interact_text()?;

    Ok(AnsiTrack::SubtitleTrack(track))
}
