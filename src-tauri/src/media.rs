use crate::jobs::{
    emit_job_progress, emit_log, is_audio_format, media_kind_for_format, JobProgressPayload,
    JobState,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Manager, Runtime, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EqualizerSettings {
    pub bass: f32,
    pub mid: f32,
    pub treble: f32,
}

#[derive(Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SplitMode {
    #[default]
    None,
    Silence,
    Chapters,
    Manual,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SilenceSplitSettings {
    #[serde(default = "default_silence_threshold_db")]
    pub threshold_db: f32,
    #[serde(default = "default_silence_min_duration")]
    pub min_silence_duration: f64,
    #[serde(default = "default_min_segment_duration")]
    pub min_segment_duration: f64,
}

impl Default for SilenceSplitSettings {
    fn default() -> Self {
        Self {
            threshold_db: default_silence_threshold_db(),
            min_silence_duration: default_silence_min_duration(),
            min_segment_duration: default_min_segment_duration(),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMediaRequest {
    pub id: String,
    pub input_path: String,
    pub output_dir: Option<String>,
    pub format: String,
    pub trim_start: Option<String>,
    pub trim_end: Option<String>,
    pub normalize: bool,
    #[serde(default = "default_loudness_target_lufs")]
    pub loudness_target_lufs: f32,
    pub eq: EqualizerSettings,
    pub audio_target: Option<String>,
    #[serde(default)]
    pub split_mode: SplitMode,
    #[serde(default)]
    pub manual_markers: Vec<f64>,
    #[serde(default)]
    pub silence: SilenceSplitSettings,
    #[serde(default)]
    pub fade_in_duration: f64,
    #[serde(default)]
    pub fade_out_duration: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaProbeResponse {
    pub input_path: String,
    pub title: String,
    pub duration_seconds: f64,
    pub duration_label: String,
    pub media_kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformPreviewResponse {
    pub input_path: String,
    pub image_data_url: String,
    pub audio_data_url: String,
    pub duration_seconds: f64,
    pub duration_label: String,
    pub media_kind: String,
}

#[derive(Clone)]
struct InputMediaInfo {
    duration_seconds: f64,
    media_kind: String,
}

#[derive(Clone)]
struct Segment {
    start_seconds: f64,
    end_seconds: f64,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn probe_media<R: Runtime>(
    app: AppHandle<R>,
    inputPath: String,
) -> Result<MediaProbeResponse, String> {
    let ffmpeg_path = resolve_ffmpeg_path(&app)?;
    let media_info = probe_media_info(&ffmpeg_path, &inputPath).await?;

    Ok(MediaProbeResponse {
        input_path: inputPath.clone(),
        title: file_label(&inputPath),
        duration_seconds: media_info.duration_seconds,
        duration_label: format_seconds(media_info.duration_seconds),
        media_kind: media_info.media_kind,
    })
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn render_waveform_preview<R: Runtime>(
    app: AppHandle<R>,
    inputPath: String,
) -> Result<WaveformPreviewResponse, String> {
    let ffmpeg_path = resolve_ffmpeg_path(&app)?;
    let media_info = probe_media_info(&ffmpeg_path, &inputPath).await?;
    let image_path = render_waveform_image(&app, &ffmpeg_path, &inputPath).await?;
    let audio_path = render_preview_audio(&app, &ffmpeg_path, &inputPath).await?;
    let image_bytes = fs::read(&image_path).map_err(|error| error.to_string())?;
    let audio_bytes = fs::read(&audio_path).map_err(|error| error.to_string())?;
    let image_data_url = format!("data:image/png;base64,{}", STANDARD.encode(image_bytes));
    let audio_data_url = format!("data:audio/mpeg;base64,{}", STANDARD.encode(audio_bytes));

    Ok(WaveformPreviewResponse {
        input_path: inputPath,
        image_data_url,
        audio_data_url,
        duration_seconds: media_info.duration_seconds,
        duration_label: format_seconds(media_info.duration_seconds),
        media_kind: media_info.media_kind,
    })
}

#[tauri::command]
pub async fn start_media_process<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: ProcessMediaRequest,
) -> Result<(), String> {
    let id = request.id.clone();
    let input_path = request.input_path.clone();
    let format = request.format.clone();
    let media_kind = media_kind_for_format(&format).to_string();
    let job_state = state.inner().clone();
    let (pid, cancelled) = job_state.register(&id)?;
    let app_handle = app.clone();

    emit_log(
        &app_handle,
        format!("Queued media processing for {}", input_path),
        "info",
    );

    tokio::spawn(async move {
        let result = run_media_process(
            app_handle.clone(),
            request.clone(),
            Arc::clone(&pid),
            Arc::clone(&cancelled),
        )
        .await;

        if let Err(error) = result {
            if cancelled.load(Ordering::SeqCst) {
                emit_job_progress(
                    &app_handle,
                    JobProgressPayload {
                        id: id.clone(),
                        job_kind: "process".into(),
                        media_kind: media_kind.clone(),
                        status: "cancelled".into(),
                        percent: 0.0,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: Some(file_label(&input_path)),
                        detail: Some("Cancelled by user".into()),
                        output_path: None,
                        error: None,
                    },
                );
                emit_log(
                    &app_handle,
                    format!("Processing cancelled for {}", input_path),
                    "warn",
                );
            } else {
                emit_job_progress(
                    &app_handle,
                    JobProgressPayload {
                        id: id.clone(),
                        job_kind: "process".into(),
                        media_kind: media_kind.clone(),
                        status: "error".into(),
                        percent: 0.0,
                        speed: "-".into(),
                        eta: "-".into(),
                        total_size: "-".into(),
                        title: Some(file_label(&input_path)),
                        detail: Some(error.clone()),
                        output_path: None,
                        error: Some(error.clone()),
                    },
                );
                emit_log(
                    &app_handle,
                    format!("Processing failed for {}: {}", input_path, error),
                    "error",
                );
            }
        }

        job_state.remove(&id);
    });

    Ok(())
}

async fn run_media_process<R: Runtime>(
    app: AppHandle<R>,
    request: ProcessMediaRequest,
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled before processing started".into());
    }

    let ffmpeg_path = resolve_ffmpeg_path(&app)?;
    let media_info = probe_media_info(&ffmpeg_path, &request.input_path).await?;
    let file_name = file_label(&request.input_path);
    let (region_start, region_end) = resolve_processing_window(
        media_info.duration_seconds,
        &request.trim_start,
        &request.trim_end,
    )?;
    let resolved_output_dir = resolve_output_dir(&request, &request.input_path)?;

    let segments = build_segments(&ffmpeg_path, &request, region_start, region_end).await?;
    let segment_count = segments.len();
    let mut final_output_path = resolved_output_dir.to_string_lossy().to_string();

    emit_job_progress(
        &app,
        JobProgressPayload {
            id: request.id.clone(),
            job_kind: "process".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "processing".into(),
            percent: 0.0,
            speed: "-".into(),
            eta: "-".into(),
            total_size: format!("{segment_count} segment(s)"),
            title: Some(file_name.clone()),
            detail: Some(format!("Prepared {segment_count} segment(s)")),
            output_path: None,
            error: None,
        },
    );

    for (segment_index, segment) in segments.iter().enumerate() {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Cancelled by user".into());
        }

        let output_path =
            build_output_path(&request, &request.input_path, segment_index, segment_count)?;

        emit_log(
            &app,
            format!(
                "Rendering segment {}/{} for {}",
                segment_index + 1,
                segment_count,
                request.input_path
            ),
            "info",
        );

        run_segment_render(
            &app,
            &request,
            &ffmpeg_path,
            segment,
            segment_index,
            segment_count,
            &output_path,
            &file_name,
            Arc::clone(&pid),
            Arc::clone(&cancelled),
        )
        .await?;

        if segment_count == 1 {
            final_output_path = output_path.to_string_lossy().to_string();
        }
    }

    emit_job_progress(
        &app,
        JobProgressPayload {
            id: request.id.clone(),
            job_kind: "process".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "done".into(),
            percent: 100.0,
            speed: "-".into(),
            eta: "00:00".into(),
            total_size: format!("{segment_count} segment(s)"),
            title: Some(file_name.clone()),
            detail: Some(if segment_count > 1 {
                format!("Split finished with {segment_count} files")
            } else {
                "Processing finished".into()
            }),
            output_path: Some(final_output_path),
            error: None,
        },
    );

    emit_log(
        &app,
        format!(
            "Processing finished for {} with {} segment(s)",
            request.input_path, segment_count
        ),
        "success",
    );

    Ok(())
}

async fn render_waveform_image<R: Runtime>(
    app: &AppHandle<R>,
    ffmpeg_path: &Path,
    input_path: &str,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("waveforms");
    fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

    let output_path = cache_dir.join(format!("waveform-{}.png", waveform_cache_key(input_path)));
    if output_path.exists() {
        return Ok(output_path.to_string_lossy().to_string());
    }

    let output = Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-y",
            "-i",
            input_path,
            "-filter_complex",
            "[0:a]aformat=channel_layouts=mono,showwavespic=s=1600x320:colors=0x10B981",
            "-frames:v",
            "1",
        ])
        .arg(output_path.as_os_str())
        .output()
        .await
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Unable to render waveform preview");
        return Err(message.to_string());
    }

    Ok(output_path.to_string_lossy().to_string())
}

async fn render_preview_audio<R: Runtime>(
    app: &AppHandle<R>,
    ffmpeg_path: &Path,
    input_path: &str,
) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("waveforms");
    fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;

    let output_path = cache_dir.join(format!("preview-{}.mp3", waveform_cache_key(input_path)));
    if output_path.exists() {
        return Ok(output_path.to_string_lossy().to_string());
    }

    let output = Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-y",
            "-i",
            input_path,
            "-vn",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "96k",
            "-ac",
            "1",
            "-ar",
            "44100",
        ])
        .arg(output_path.as_os_str())
        .output()
        .await
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Unable to render preview audio");
        return Err(message.to_string());
    }

    Ok(output_path.to_string_lossy().to_string())
}

async fn run_segment_render<R: Runtime>(
    app: &AppHandle<R>,
    request: &ProcessMediaRequest,
    ffmpeg_path: &Path,
    segment: &Segment,
    segment_index: usize,
    segment_count: usize,
    output_path: &Path,
    file_name: &str,
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), String> {
    let segment_duration = (segment.end_seconds - segment.start_seconds).max(0.0);
    let filter_chain = build_filter_chain(request, segment_duration);

    let mut args = vec![
        "-hide_banner".to_string(),
        "-nostdin".to_string(),
        "-y".to_string(),
    ];

    if segment.start_seconds > 0.0 {
        args.push("-ss".to_string());
        args.push(format_ffmpeg_seconds(segment.start_seconds));
    }

    args.push("-i".to_string());
    args.push(request.input_path.clone());
    args.push("-t".to_string());
    args.push(format_ffmpeg_seconds(segment_duration));

    if !filter_chain.is_empty() {
        args.push("-af".to_string());
        args.push(filter_chain);
    }

    apply_output_format_args(&mut args, &request.format, request.audio_target.as_deref());

    args.push("-progress".to_string());
    args.push("pipe:1".to_string());
    args.push("-nostats".to_string());
    args.push(output_path.to_string_lossy().to_string());

    let mut child = Command::new(ffmpeg_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    if let Ok(mut guard) = pid.lock() {
        *guard = child.id();
    }

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stderr".to_string())?;

    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut buffer = Vec::new();

        while let Ok(Some(line)) = lines.next_line().await {
            buffer.push(line);
        }

        buffer
    });

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stdout".to_string())?;
    let mut lines = BufReader::new(stdout).lines();

    let mut speed = "-".to_string();
    let output_label = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| output_path.to_string_lossy().to_string());

    while let Ok(Some(line)) = lines.next_line().await {
        if cancelled.load(Ordering::SeqCst) {
            return Err("Cancelled by user".into());
        }

        if let Some((key, value)) = line.split_once('=') {
            match key {
                "speed" => {
                    speed = value.trim().to_string();
                }
                "out_time" => {
                    let out_time = parse_ffmpeg_time(value).unwrap_or(0.0);
                    let segment_progress = if segment_duration > 0.0 {
                        ((out_time / segment_duration) * 100.0).clamp(0.0, 99.5)
                    } else {
                        0.0
                    };
                    let overall_percent = (((segment_index as f64) + (segment_progress / 100.0))
                        / segment_count as f64)
                        * 100.0;

                    emit_job_progress(
                        app,
                        JobProgressPayload {
                            id: request.id.clone(),
                            job_kind: "process".into(),
                            media_kind: media_kind_for_format(&request.format).into(),
                            status: "processing".into(),
                            percent: overall_percent,
                            speed: speed.clone(),
                            eta: format_seconds((segment_duration - out_time).max(0.0)),
                            total_size: format!("{}/{}", segment_index + 1, segment_count),
                            title: Some(file_name.to_string()),
                            detail: Some(format!(
                                "Segment {}/{} -> {}",
                                segment_index + 1,
                                segment_count,
                                output_label
                            )),
                            output_path: Some(output_path.to_string_lossy().to_string()),
                            error: None,
                        },
                    );
                }
                "progress" if value.trim() == "end" => {
                    break;
                }
                _ => {}
            }
        }
    }

    let status = child.wait().await.map_err(|error| error.to_string())?;
    let stderr_lines = stderr_task.await.map_err(|error| error.to_string())?;

    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }

    if !status.success() {
        let stderr_excerpt = stderr_lines
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "ffmpeg exited with an error".into());
        return Err(stderr_excerpt);
    }

    Ok(())
}

async fn build_segments(
    ffmpeg_path: &Path,
    request: &ProcessMediaRequest,
    region_start: f64,
    region_end: f64,
) -> Result<Vec<Segment>, String> {
    match request.split_mode {
        SplitMode::None => Ok(vec![Segment {
            start_seconds: region_start,
            end_seconds: region_end,
        }]),
        SplitMode::Manual => {
            build_manual_segments(region_start, region_end, &request.manual_markers)
        }
        SplitMode::Chapters => {
            let chapter_segments =
                build_chapter_segments(ffmpeg_path, request, region_start, region_end).await?;
            if chapter_segments.len() > 1 {
                Ok(chapter_segments)
            } else {
                build_silence_segments(ffmpeg_path, request, region_start, region_end).await
            }
        }
        SplitMode::Silence => {
            build_silence_segments(ffmpeg_path, request, region_start, region_end).await
        }
    }
}

fn build_manual_segments(
    region_start: f64,
    region_end: f64,
    manual_markers: &[f64],
) -> Result<Vec<Segment>, String> {
    let mut boundaries = vec![region_start];
    let mut markers: Vec<f64> = manual_markers
        .iter()
        .copied()
        .filter(|marker| *marker > region_start && *marker < region_end)
        .collect();
    markers.sort_by(|left, right| left.total_cmp(right));
    markers.dedup_by(|left, right| (*left - *right).abs() < 0.05);

    if markers.is_empty() {
        return Err("Add at least one manual marker to use manual split mode.".into());
    }

    boundaries.extend(markers);
    boundaries.push(region_end);

    segments_from_boundaries(&boundaries)
}

async fn build_chapter_segments(
    ffmpeg_path: &Path,
    request: &ProcessMediaRequest,
    region_start: f64,
    region_end: f64,
) -> Result<Vec<Segment>, String> {
    let output = Command::new(ffmpeg_path)
        .args(["-hide_banner", "-i", &request.input_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| error.to_string())?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let chapter_re = Regex::new(
        r"Chapter\s+#\d+:\d+:\s+start\s+([0-9]+(?:\.[0-9]+)?),\s+end\s+([0-9]+(?:\.[0-9]+)?)",
    )
    .unwrap();

    let mut boundaries = vec![region_start];
    let mut chapter_starts: Vec<f64> = chapter_re
        .captures_iter(&stderr)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<f64>().ok())
        .filter(|start| *start > region_start + 0.25 && *start < region_end - 0.25)
        .collect();

    chapter_starts.sort_by(|left, right| left.total_cmp(right));
    chapter_starts.dedup_by(|left, right| (*left - *right).abs() < 0.25);

    for chapter_start in chapter_starts {
        let last_boundary = *boundaries.last().unwrap_or(&region_start);
        if chapter_start - last_boundary >= request.silence.min_segment_duration
            && region_end - chapter_start >= request.silence.min_segment_duration
        {
            boundaries.push(chapter_start);
        }
    }

    boundaries.push(region_end);
    segments_from_boundaries(&boundaries)
}

async fn build_silence_segments(
    ffmpeg_path: &Path,
    request: &ProcessMediaRequest,
    region_start: f64,
    region_end: f64,
) -> Result<Vec<Segment>, String> {
    let output = Command::new(ffmpeg_path)
        .args([
            "-hide_banner",
            "-nostdin",
            "-i",
            &request.input_path,
            "-af",
            &format!(
                "silencedetect=n={}dB:d={}",
                request.silence.threshold_db, request.silence.min_silence_duration
            ),
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| error.to_string())?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let re_start = Regex::new(r"silence_start:\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
    let re_end = Regex::new(r"silence_end:\s*([0-9]+(?:\.[0-9]+)?)").unwrap();

    let mut boundaries = vec![region_start];
    let mut active_silence_start = None::<f64>;

    for line in stderr.lines() {
        if let Some(captures) = re_start.captures(line) {
            active_silence_start = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok());
            continue;
        }

        if let Some(captures) = re_end.captures(line) {
            let silence_end = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<f64>().ok());

            if let (Some(silence_start), Some(silence_end)) = (active_silence_start, silence_end) {
                let midpoint = (silence_start + silence_end) / 2.0;
                let last_boundary = *boundaries.last().unwrap_or(&region_start);

                if midpoint > region_start
                    && midpoint < region_end
                    && midpoint - last_boundary >= request.silence.min_segment_duration
                    && region_end - midpoint >= request.silence.min_segment_duration
                {
                    boundaries.push(midpoint);
                }
            }

            active_silence_start = None;
        }
    }

    boundaries.push(region_end);

    let segments = segments_from_boundaries(&boundaries)?;

    if segments.is_empty() {
        return Ok(vec![Segment {
            start_seconds: region_start,
            end_seconds: region_end,
        }]);
    }

    Ok(segments)
}

fn segments_from_boundaries(boundaries: &[f64]) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();

    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];

        if end - start > 0.25 {
            segments.push(Segment {
                start_seconds: start,
                end_seconds: end,
            });
        }
    }

    if segments.is_empty() {
        return Err("No valid output segments were produced.".into());
    }

    Ok(segments)
}

fn resolve_processing_window(
    total_duration: f64,
    trim_start: &Option<String>,
    trim_end: &Option<String>,
) -> Result<(f64, f64), String> {
    let start = trim_start
        .as_ref()
        .and_then(|value| parse_user_time(value))
        .unwrap_or(0.0)
        .clamp(0.0, total_duration);

    let end = trim_end
        .as_ref()
        .and_then(|value| parse_user_time(value))
        .unwrap_or(total_duration)
        .clamp(0.0, total_duration);

    if end <= start {
        return Err("Trim end must be greater than trim start.".into());
    }

    Ok((start, end))
}

fn resolve_ffmpeg_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let bin_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("bin");

    let ffmpeg_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };

    let ffmpeg_path = bin_dir.join(ffmpeg_name);

    if ffmpeg_path.exists() {
        Ok(ffmpeg_path)
    } else {
        Err("FFmpeg binary is missing. Install it from Settings first.".into())
    }
}

async fn probe_media_info(ffmpeg_path: &Path, input_path: &str) -> Result<InputMediaInfo, String> {
    let output = Command::new(ffmpeg_path)
        .args(["-hide_banner", "-i", input_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|error| error.to_string())?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let duration_re = Regex::new(r"Duration:\s+(\d{2}:\d{2}:\d{2}\.\d+)").unwrap();
    let video_re = Regex::new(r"\bVideo:\b").unwrap();

    let duration_seconds = duration_re
        .captures(&stderr)
        .and_then(|captures| captures.get(1))
        .and_then(|value| parse_ffmpeg_time(value.as_str()))
        .ok_or_else(|| "Could not read media duration from ffmpeg".to_string())?;

    let media_kind = if video_re.is_match(&stderr) {
        "video"
    } else {
        "audio"
    };

    Ok(InputMediaInfo {
        duration_seconds,
        media_kind: media_kind.to_string(),
    })
}

fn build_output_path(
    request: &ProcessMediaRequest,
    input_path: &str,
    segment_index: usize,
    segment_count: usize,
) -> Result<PathBuf, String> {
    let input = Path::new(input_path);
    let output_dir = resolve_output_dir(request, input_path)?;

    let stem = input
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("processed");

    let output_format = effective_audio_format(&request.format, request.audio_target.as_deref());

    let file_name = if segment_count > 1 {
        format!("{}_part{:02}.{}", stem, segment_index + 1, output_format)
    } else {
        format!("{}_processed.{}", stem, output_format)
    };

    Ok(output_dir.join(file_name))
}

fn resolve_output_dir(request: &ProcessMediaRequest, input_path: &str) -> Result<PathBuf, String> {
    let input = Path::new(input_path);

    request
        .output_dir
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| input.parent().map(PathBuf::from))
        .ok_or_else(|| "Unable to determine output folder".to_string())
}

fn effective_audio_format<'a>(format: &'a str, audio_target: Option<&str>) -> &'a str {
    if is_audio_format(format) && is_azuracast_target(audio_target) {
        "mp3"
    } else {
        format
    }
}

fn build_filter_chain(request: &ProcessMediaRequest, segment_duration: f64) -> String {
    let mut filters = Vec::new();

    if request.normalize {
        filters.push(format!(
            "loudnorm=I={}:TP=-1.5:LRA=11",
            request.loudness_target_lufs.clamp(-30.0, -6.0)
        ));
    }

    if request.eq.bass.abs() > 0.01 {
        filters.push(format!("equalizer=f=120:t=q:w=1.0:g={}", request.eq.bass));
    }

    if request.eq.mid.abs() > 0.01 {
        filters.push(format!("equalizer=f=1000:t=q:w=1.0:g={}", request.eq.mid));
    }

    if request.eq.treble.abs() > 0.01 {
        filters.push(format!(
            "equalizer=f=8000:t=q:w=1.0:g={}",
            request.eq.treble
        ));
    }

    let max_fade = (segment_duration / 2.0).max(0.0);
    let fade_in = request.fade_in_duration.max(0.0).min(max_fade);
    let fade_out = request.fade_out_duration.max(0.0).min(max_fade);

    if fade_in > 0.0 {
        filters.push(format!(
            "afade=t=in:st=0:d={}",
            format_ffmpeg_seconds(fade_in)
        ));
    }

    if fade_out > 0.0 {
        filters.push(format!(
            "afade=t=out:st={}:d={}",
            format_ffmpeg_seconds((segment_duration - fade_out).max(0.0)),
            format_ffmpeg_seconds(fade_out)
        ));
    }

    filters.join(",")
}

fn apply_output_format_args(args: &mut Vec<String>, format: &str, audio_target: Option<&str>) {
    if is_audio_format(format) {
        args.push("-vn".to_string());
        let azuracast_target = is_azuracast_target(audio_target);

        match if azuracast_target { "mp3" } else { format } {
            "mp3" => {
                args.extend(
                    ["-c:a", "libmp3lame", "-b:a", "320k"]
                        .iter()
                        .map(ToString::to_string),
                );
            }
            "flac" => {
                args.extend(["-c:a", "flac"].iter().map(ToString::to_string));
            }
            "ogg" => {
                args.extend(
                    ["-c:a", "libvorbis", "-q:a", "6"]
                        .iter()
                        .map(ToString::to_string),
                );
            }
            "wav" => {
                args.extend(["-c:a", "pcm_s16le"].iter().map(ToString::to_string));
            }
            "m4a" | "aac" => {
                args.extend(
                    ["-c:a", "aac", "-b:a", "256k"]
                        .iter()
                        .map(ToString::to_string),
                );
            }
            _ => {
                args.extend(["-c:a", "copy"].iter().map(ToString::to_string));
            }
        }

        if azuracast_target {
            args.extend(
                [
                    "-ar",
                    "44100",
                    "-ac",
                    "2",
                    "-id3v2_version",
                    "3",
                    "-write_id3v1",
                    "1",
                ]
                .iter()
                .map(ToString::to_string),
            );
        }

        return;
    }

    args.extend(
        ["-map", "0:v:0?", "-map", "0:a:0?"]
            .iter()
            .map(ToString::to_string),
    );

    match format {
        "webm" => {
            args.extend(
                [
                    "-c:v",
                    "libvpx-vp9",
                    "-crf",
                    "32",
                    "-b:v",
                    "0",
                    "-c:a",
                    "libopus",
                    "-b:a",
                    "160k",
                ]
                .iter()
                .map(ToString::to_string),
            );
        }
        "mkv" => {
            args.extend(
                [
                    "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-c:a", "aac", "-b:a",
                    "192k",
                ]
                .iter()
                .map(ToString::to_string),
            );
        }
        _ => {
            args.extend(
                [
                    "-c:v",
                    "libx264",
                    "-preset",
                    "medium",
                    "-crf",
                    "20",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "192k",
                    "-movflags",
                    "+faststart",
                ]
                .iter()
                .map(ToString::to_string),
            );
        }
    }
}

fn is_azuracast_target(target: Option<&str>) -> bool {
    matches!(target, Some("azuracast"))
}

fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    parse_user_time(value)
}

fn parse_user_time(value: &str) -> Option<f64> {
    let raw = value.trim();
    if raw.is_empty() {
        return None;
    }

    if !raw.contains(':') {
        return raw.parse::<f64>().ok();
    }

    let mut seconds = 0.0_f64;

    for (index, part) in raw.split(':').rev().enumerate() {
        let multiplier = match index {
            0 => 1.0,
            1 => 60.0,
            2 => 3600.0,
            _ => return None,
        };

        seconds += part.parse::<f64>().ok()? * multiplier;
    }

    Some(seconds)
}

fn format_seconds(value: f64) -> String {
    let total = value.max(0.0).round() as u64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn format_ffmpeg_seconds(value: f64) -> String {
    format!("{:.3}", value.max(0.0))
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

fn waveform_cache_key(input_path: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    input_path.hash(&mut hasher);

    if let Ok(metadata) = fs::metadata(input_path) {
        metadata.len().hash(&mut hasher);

        if let Ok(modified) = metadata.modified() {
            if let Ok(since_epoch) = modified.duration_since(UNIX_EPOCH) {
                since_epoch.as_secs().hash(&mut hasher);
                since_epoch.subsec_nanos().hash(&mut hasher);
            }
        }
    }

    hasher.finish()
}

fn default_silence_threshold_db() -> f32 {
    -35.0
}

fn default_silence_min_duration() -> f64 {
    1.5
}

fn default_min_segment_duration() -> f64 {
    20.0
}

fn default_loudness_target_lufs() -> f32 {
    -16.0
}
