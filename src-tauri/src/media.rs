use crate::encoding::EncodingSettings;
use crate::jobs::{
    emit_job_progress, emit_log, is_audio_format, is_video_format, media_kind_for_format,
    JobProgressPayload, JobState,
};
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
    Duration,
    Manual,
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SplitNormalization {
    #[default]
    None,
    Before,
    After,
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
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub sensitivity: SilenceSensitivity,
}

#[derive(Deserialize, Clone, Copy, Default)]
#[serde(rename_all = "lowercase")]
pub enum SilenceSensitivity {
    Conservative,
    #[default]
    Balanced,
    Sensitive,
}

impl Default for SilenceSplitSettings {
    fn default() -> Self {
        Self {
            threshold_db: default_silence_threshold_db(),
            min_silence_duration: default_silence_min_duration(),
            min_segment_duration: default_min_segment_duration(),
            automatic: false,
            sensitivity: SilenceSensitivity::Balanced,
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
    #[serde(default = "default_true_peak")]
    pub true_peak_dbtp: f32,
    pub eq: EqualizerSettings,
    pub audio_target: Option<String>,
    pub(crate) encoding: Option<EncodingSettings>,
    #[serde(default)]
    pub split_mode: SplitMode,
    pub split_duration_seconds: Option<f64>,
    pub split_pause_window_seconds: Option<f64>,
    pub output_name_pattern: Option<String>,
    #[serde(default)]
    pub split_normalization: SplitNormalization,
    #[serde(default)]
    pub manual_markers: Vec<f64>,
    #[serde(default)]
    pub silence: SilenceSplitSettings,
    #[serde(default)]
    pub fade_in_duration: f64,
    #[serde(default)]
    pub fade_out_duration: f64,
}

impl ProcessMediaRequest {
    fn normalizes_each_segment(&self) -> bool {
        self.normalize || self.split_normalization == SplitNormalization::After
    }
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
    pub audio_path: String,
    pub duration_seconds: f64,
    pub duration_label: String,
    pub media_kind: String,
}

#[derive(Clone)]
struct InputMediaInfo {
    duration_seconds: f64,
    media_kind: String,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    start_seconds: f64,
    end_seconds: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPlan {
    segments: Vec<Segment>,
    automatic: Option<AutomaticSilence>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AutomaticSilence {
    threshold_db: f32,
    min_silence_duration: f64,
    pause_count: usize,
    pauses: Vec<Segment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitComparison {
    original_path: String,
    processed_path: String,
}

struct NormalizedAudio {
    path: PathBuf,
    start_seconds: f64,
}

impl Drop for NormalizedAudio {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
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
    state: State<'_, JobState>,
    inputPath: String,
    id: String,
) -> Result<WaveformPreviewResponse, String> {
    let (pid, cancelled) = state.register(&id)?;
    let result = async {
        let ffmpeg_path = resolve_ffmpeg_path(&app)?;
        let media_info = probe_media_info(&ffmpeg_path, &inputPath).await?;
        let audio_path =
            render_preview_audio(&app, &ffmpeg_path, &inputPath, &pid, &cancelled).await?;
        Ok(WaveformPreviewResponse {
            input_path: inputPath,
            audio_path,
            duration_seconds: media_info.duration_seconds,
            duration_label: format_seconds(media_info.duration_seconds),
            media_kind: media_info.media_kind,
        })
    }
    .await;
    state.remove(&id);
    result
}

#[tauri::command]
pub async fn start_media_process<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: ProcessMediaRequest,
) -> Result<(), String> {
    validate_request(&request)?;
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

    fs::create_dir_all(&resolved_output_dir).map_err(|e| e.to_string())?;
    let segments = plan_segments(
        &ffmpeg_path,
        &request,
        region_start,
        region_end,
        waveform_cache_dir(&app).as_deref(),
        &pid,
        &cancelled,
    )
    .await?
    .segments;
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

    let normalized_audio = if request.split_normalization == SplitNormalization::Before {
        emit_loudness_progress(&app, &request, &file_name, 0.0, "Normalizando la selección");
        let selection = Segment {
            start_seconds: region_start,
            end_seconds: region_end,
        };
        prepare_normalized_audio(
            &ffmpeg_path,
            &request,
            &selection,
            &resolved_output_dir,
            &pid,
            &cancelled,
        )
        .await?
    } else {
        None
    };

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

        let staging_path = reserve_staging_path(&output_path)?;
        let render_result = run_segment_render(
            &app,
            &request,
            &ffmpeg_path,
            segment,
            segment_index,
            segment_count,
            &staging_path,
            &file_name,
            normalized_audio.as_ref(),
            Arc::clone(&pid),
            Arc::clone(&cancelled),
        )
        .await;
        if let Err(error) = render_result {
            let _ = fs::remove_file(&staging_path);
            return Err(error);
        }
        let published = publish_export(&staging_path, &request, segment_index, segment_count);
        let _ = fs::remove_file(&staging_path);
        let output_path = published?;

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

async fn render_preview_audio<R: Runtime>(
    app: &AppHandle<R>,
    ffmpeg_path: &Path,
    input_path: &str,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
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

    let staging_path = reserve_staging_path(&output_path)?;
    let mut command = Command::new(ffmpeg_path);
    command
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
        .arg(staging_path.as_os_str())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = match tracked_output(&mut command, pid, cancelled).await {
        Ok(output) => output,
        Err(error) => {
            let _ = fs::remove_file(&staging_path);
            return Err(error);
        }
    };

    if !output.status.success() {
        let _ = fs::remove_file(&staging_path);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Unable to render preview audio");
        return Err(message.to_string());
    }

    if !output_path.exists() {
        fs::rename(&staging_path, &output_path).map_err(|e| e.to_string())?;
    } else {
        let _ = fs::remove_file(&staging_path);
    }
    Ok(output_path.to_string_lossy().to_string())
}

fn emit_loudness_progress<R: Runtime>(
    app: &AppHandle<R>,
    request: &ProcessMediaRequest,
    file_name: &str,
    percent: f64,
    detail: &str,
) {
    emit_job_progress(
        app,
        JobProgressPayload {
            id: request.id.clone(),
            job_kind: "process".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "processing".into(),
            percent,
            speed: "-".into(),
            eta: "-".into(),
            total_size: "-".into(),
            title: Some(file_name.into()),
            detail: Some(detail.into()),
            output_path: None,
            error: None,
        },
    );
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
    normalized_audio: Option<&NormalizedAudio>,
    pid: Arc<Mutex<Option<u32>>>,
    cancelled: Arc<AtomicBool>,
) -> Result<(), String> {
    let segment_duration = (segment.end_seconds - segment.start_seconds).max(0.0);
    if request.normalizes_each_segment() {
        emit_loudness_progress(
            app,
            request,
            file_name,
            segment_index as f64 / segment_count as f64 * 100.0,
            &format!(
                "Midiendo sonoridad · segmento {}/{} · pasada 1/2",
                segment_index + 1,
                segment_count
            ),
        );
    }

    let filter_chain = segment_audio_filter(
        ffmpeg_path,
        request,
        segment,
        normalized_audio.is_some(),
        &pid,
        &cancelled,
    )
    .await?;
    let args = segment_render_args(
        request,
        segment,
        &filter_chain,
        output_path,
        normalized_audio,
    );

    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }
    let mut child = Command::new(ffmpeg_path)
        .kill_on_drop(true)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;

    if let Ok(mut guard) = pid.lock() {
        *guard = child.id();
    }

    if cancelled.load(Ordering::SeqCst) {
        let _ = child.kill().await;
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
            let _ = child.kill().await;
            let _ = stderr_task.await;
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
    if let Ok(mut guard) = pid.lock() {
        *guard = None;
    }
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
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<Segment>, String> {
    match request.split_mode {
        SplitMode::None => Ok(vec![Segment {
            start_seconds: region_start,
            end_seconds: region_end,
        }]),
        SplitMode::Manual => {
            build_manual_segments(region_start, region_end, &request.manual_markers)
        }
        SplitMode::Duration => {
            let segments =
                build_duration_segments(region_start, region_end, request.split_duration_seconds)?;
            let window = request.split_pause_window_seconds.unwrap_or(0.0);
            if window == 0.0 {
                return Ok(segments);
            }
            let pauses = detect_silences(ffmpeg_path, request, pid, cancelled).await?;
            snap_duration_cuts(&segments, &pauses, window)
        }
        SplitMode::Chapters => {
            let segments = build_chapter_segments(
                ffmpeg_path,
                request,
                region_start,
                region_end,
                pid,
                cancelled,
            )
            .await?;
            if segments.len() < 2 {
                Err("No hay capítulos utilizables dentro de la selección. Prueba el modo por silencios o añade cortes manuales.".into())
            } else {
                Ok(segments)
            }
        }
        SplitMode::Silence => {
            build_silence_segments(
                ffmpeg_path,
                request,
                region_start,
                region_end,
                pid,
                cancelled,
            )
            .await
        }
    }
}

async fn plan_segments(
    ffmpeg: &Path,
    request: &ProcessMediaRequest,
    start: f64,
    end: f64,
    cache: Option<&Path>,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<SplitPlan, String> {
    let snap_window = request.split_pause_window_seconds.unwrap_or(0.0);
    let snap = request.split_mode == SplitMode::Duration && snap_window > 0.0;
    if !request.silence.automatic || (!snap && request.split_mode != SplitMode::Silence) {
        let segments = build_segments(ffmpeg, request, start, end, pid, cancelled).await?;
        return Ok(SplitPlan {
            segments,
            automatic: None,
        });
    }

    let levels = cached_waveform_levels(ffmpeg, &request.input_path, cache, pid, cancelled).await?;
    let automatic = automatic_pauses(&levels, start, end, request.silence.sensitivity);
    let segments = if snap {
        let nominal = build_duration_segments(start, end, request.split_duration_seconds)?;
        snap_duration_cuts(&nominal, &automatic.pauses, snap_window)?
    } else {
        silence_segments_from_pauses(
            start,
            end,
            request.silence.min_segment_duration,
            &automatic.pauses,
        )?
    };
    Ok(SplitPlan {
        segments,
        automatic: Some(automatic),
    })
}

fn level_percentile(histogram: &[usize; 133], frames: usize, fraction: f64) -> f32 {
    let rank = ((frames.saturating_sub(1)) as f64 * fraction).floor() as usize;
    let mut cumulative = 0;
    for (index, count) in histogram.iter().enumerate() {
        cumulative += count;
        if cumulative > rank {
            return index as f32 - 120.0;
        }
    }
    -120.0
}

fn automatic_pauses(
    levels: &WaveformLevelsResponse,
    start: f64,
    end: f64,
    sensitivity: SilenceSensitivity,
) -> AutomaticSilence {
    let interval = levels.interval_seconds;
    let len = levels.peaks_db.len();
    let first = ((start / interval).floor() as usize).min(len);
    let last = ((end / interval).ceil() as usize).min(len);
    let mut histogram = [0; 133];
    for peak in &levels.peaks_db[first..last] {
        histogram[(peak.round().clamp(-120.0, 12.0) + 120.0) as usize] += 1;
    }
    // Use a quiet tail and the typical active signal. Even on material with a few
    // quiet windows this leaves room between room tone and low-level wanted audio.
    let frames = last - first;
    let active = level_percentile(&histogram, frames, 0.85);
    let quiet = level_percentile(&histogram, frames, 0.02);
    let (adjustment, min_silence_duration) = match sensitivity {
        SilenceSensitivity::Conservative => (-4.0, 0.8),
        SilenceSensitivity::Balanced => (0.0, 0.45),
        SilenceSensitivity::Sensitive => (4.0, 0.25),
    };
    let baseline = if active - quiet >= 10.0 {
        (quiet + 6.0).clamp(active - 32.0, active - 8.0)
    } else {
        active - 17.0
    };
    let threshold_db = (baseline + adjustment).min(active - 6.0).clamp(-80.0, -5.0);

    let mut pauses = Vec::new();
    if active > -75.0 {
        let mut opening = None;
        let mut quiet_end = 0;
        for frame in first..=last {
            let hysteresis = if opening.is_some() { 2.0 } else { 0.0 };
            let is_quiet = frame < last
                && levels.peaks_db[frame] <= threshold_db + hysteresis
                && levels.rms_db[frame] <= threshold_db - 3.0 + hysteresis;
            if is_quiet {
                opening.get_or_insert(frame);
                quiet_end = frame + 1;
            } else if frame == last || frame >= quiet_end + 2 {
                if let Some(open) = opening.take() {
                    let pause_start = (open as f64 * interval).max(start);
                    let pause_end = (quiet_end as f64 * interval).min(end);
                    if pause_end - pause_start >= min_silence_duration {
                        pauses.push(Segment {
                            start_seconds: pause_start,
                            end_seconds: pause_end,
                        });
                    }
                }
            }
        }
    }
    AutomaticSilence {
        threshold_db,
        min_silence_duration,
        pause_count: pauses.len(),
        pauses,
    }
}

fn snap_duration_cuts(
    segments: &[Segment],
    pauses: &[Segment],
    window: f64,
) -> Result<Vec<Segment>, String> {
    let start = segments[0].start_seconds;
    let end = segments.last().unwrap().end_seconds;
    let mut boundaries = vec![start];
    for (index, segment) in segments.iter().enumerate().skip(1) {
        let cut = segment.start_seconds;
        // Give adjacent cuts separate search windows so they cannot cross or collapse.
        let low = if index == 1 {
            start + 0.001
        } else {
            (segments[index - 1].start_seconds + cut) / 2.0 + 0.0005
        };
        let high = if index + 1 == segments.len() {
            end - 0.001
        } else {
            (cut + segment.end_seconds) / 2.0 - 0.0005
        };
        let low = ((cut - window).max(low) * 1000.0 - 1e-8).ceil();
        let high = ((cut + window).min(high) * 1000.0 + 1e-8).floor();
        let closest = pauses
            .iter()
            .filter_map(|pause| {
                let left = (pause.start_seconds * 1000.0).ceil().max(low);
                let right = (pause.end_seconds * 1000.0).floor().min(high);
                if left > right {
                    return None;
                }
                let center = ((pause.start_seconds + pause.end_seconds) * 500.0).round();
                let candidate = if pause.start_seconds <= cut && cut <= pause.end_seconds {
                    (cut * 1000.0).round()
                } else {
                    center
                };
                Some(candidate.clamp(left, right) / 1000.0)
            })
            .min_by(|a, b| (a - cut).abs().total_cmp(&(b - cut).abs()));
        boundaries.push(closest.unwrap_or(cut));
    }
    boundaries.push(end);
    segments_from_boundaries(&boundaries)
}

fn build_duration_segments(
    region_start: f64,
    region_end: f64,
    duration_seconds: Option<f64>,
) -> Result<Vec<Segment>, String> {
    let duration_ms = duration_seconds
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.001)
        .map(|seconds| (seconds * 1000.0).round() as u64)
        .ok_or("Introduce una duración de al menos 0.001 s por segmento.")?;
    let start_ms = (region_start * 1000.0).round() as u64;
    let end_ms = (region_end * 1000.0).round() as u64;
    let count = (end_ms - start_ms).div_ceil(duration_ms);
    if count < 2 {
        return Err("La duración por segmento debe ser menor que la selección.".into());
    }
    if count > 1000 {
        return Err(
            "La división supera los 1000 segmentos. Aumenta la duración por segmento.".into(),
        );
    }
    let boundaries: Vec<_> = (0..count)
        .map(|index| (start_ms + index * duration_ms) as f64 / 1000.0)
        .chain(std::iter::once(end_ms as f64 / 1000.0))
        .collect();
    segments_from_boundaries(&boundaries)
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
    markers.dedup_by(|left, right| (*left - *right).abs() < 0.000999);

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
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<Segment>, String> {
    let mut command = Command::new(ffmpeg_path);
    command
        .args(["-hide_banner", "-i", &request.input_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = tracked_output(&mut command, pid, cancelled).await?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let chapter_re = Regex::new(
        r"Chapter\s+#\d+:\d+:\s+start\s+([0-9]+(?:\.[0-9]+)?),\s+end\s+([0-9]+(?:\.[0-9]+)?)",
    )
    .unwrap();

    let mut boundaries = vec![region_start];
    let mut chapter_starts: Vec<f64> = chapter_re
        .captures_iter(&stderr)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<f64>().ok())
        .filter(|start| *start >= region_start + 0.001 && *start <= region_end - 0.001)
        .collect();

    chapter_starts.sort_by(|left, right| left.total_cmp(right));
    chapter_starts.dedup_by(|left, right| (*left - *right).abs() < 0.001);

    boundaries.extend(chapter_starts);

    boundaries.push(region_end);
    segments_from_boundaries(&boundaries)
}

async fn build_silence_segments(
    ffmpeg_path: &Path,
    request: &ProcessMediaRequest,
    region_start: f64,
    region_end: f64,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<Segment>, String> {
    let pauses = detect_silences(ffmpeg_path, request, pid, cancelled).await?;
    silence_segments_from_pauses(
        region_start,
        region_end,
        request.silence.min_segment_duration,
        &pauses,
    )
}

fn silence_segments_from_pauses(
    region_start: f64,
    region_end: f64,
    min_segment_duration: f64,
    pauses: &[Segment],
) -> Result<Vec<Segment>, String> {
    let mut boundaries = vec![region_start];
    for pause in pauses {
        let midpoint = (pause.start_seconds + pause.end_seconds) / 2.0;
        let last_boundary = *boundaries.last().unwrap_or(&region_start);
        if midpoint > region_start
            && midpoint < region_end
            && midpoint - last_boundary >= min_segment_duration
            && region_end - midpoint >= min_segment_duration
        {
            boundaries.push(midpoint);
        }
    }
    boundaries.push(region_end);
    segments_from_boundaries(&boundaries)
}

async fn detect_silences(
    ffmpeg_path: &Path,
    request: &ProcessMediaRequest,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Vec<Segment>, String> {
    let mut command = Command::new(ffmpeg_path);
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-i",
            &request.input_path,
            "-map",
            "0:a:0",
            "-vn",
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
        .stderr(Stdio::piped());
    let output = tracked_output(&mut command, pid, cancelled).await?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let re_start = Regex::new(r"silence_start:\s*([0-9]+(?:\.[0-9]+)?)").unwrap();
    let re_end = Regex::new(r"silence_end:\s*([0-9]+(?:\.[0-9]+)?)").unwrap();

    let mut pauses = Vec::new();
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
                pauses.push(Segment {
                    start_seconds: silence_start,
                    end_seconds: silence_end,
                });
            }

            active_silence_start = None;
        }
    }

    Ok(pauses)
}

fn segments_from_boundaries(boundaries: &[f64]) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();

    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];

        if end.is_finite() && start.is_finite() && end - start >= 0.000999 {
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
    if !total_duration.is_finite() || total_duration <= 0.0 {
        return Err("Duración inválida.".into());
    }
    let read = |value: &Option<String>, fallback: f64| -> Result<f64, String> {
        match value.as_deref().filter(|v| !v.trim().is_empty()) {
            Some(raw) => parse_user_time(raw).ok_or_else(|| format!("Tiempo inválido: {raw}")),
            None => Ok(fallback),
        }
    };
    let start = read(trim_start, 0.0)?;
    let end = read(trim_end, total_duration)?;
    if start < 0.0 || end > total_duration + 0.001 || end - start < 0.001 {
        return Err(
            "El inicio y el final deben formar un intervalo válido dentro del archivo.".into(),
        );
    }
    Ok((start, end))
}

fn resolve_ffmpeg_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    crate::binaries::ffmpeg_path(app)
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
    let video_re = Regex::new(r"\bVideo:").unwrap();

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

    let file_name = if let Some(pattern) = request.output_name_pattern.as_deref() {
        let name = split_output_name(pattern, stem, segment_index)?;
        format!("{name}.{output_format}")
    } else if segment_count > 1 {
        format!("{}_part{:02}.{}", stem, segment_index + 1, output_format)
    } else {
        format!(
            "{}_{}.{}",
            stem,
            if request.normalize {
                "normalized"
            } else {
                "trim"
            },
            output_format
        )
    };

    let candidate = output_dir.join(&file_name);
    if !candidate.exists() {
        return Ok(candidate);
    }
    let base = Path::new(&file_name).file_stem().unwrap().to_string_lossy();
    for suffix in 2..10000 {
        let candidate = output_dir.join(format!("{base}_{suffix}.{output_format}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err("No se encontró un nombre de exportación disponible.".into())
}

fn split_output_name(pattern: &str, stem: &str, segment_index: usize) -> Result<String, String> {
    let pattern = pattern.trim();
    let invalid_char = |ch: char| ch.is_control() || "/\\:*?\"<>|".contains(ch);
    let literals = pattern.replace("{archivo}", "").replace("{numero}", "");
    if pattern.len() > 100
        || !pattern.contains("{numero}")
        || literals
            .chars()
            .any(|ch| invalid_char(ch) || "{}".contains(ch))
    {
        return Err("El nombre de salida requiere {numero} y admite {archivo}. Evita los caracteres reservados.".into());
    }

    let stem_limit = (200 - pattern.len()) / pattern.matches("{archivo}").count().max(1);
    let mut sanitized: String = stem
        .chars()
        .map(|ch| {
            if invalid_char(ch) || "{}".contains(ch) {
                '_'
            } else {
                ch
            }
        })
        .collect();
    while sanitized.len() > stem_limit {
        sanitized.pop();
    }
    let name = pattern
        .replace("{archivo}", &sanitized)
        .replace("{numero}", &format!("{:02}", segment_index + 1));
    let head = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    if matches!(head.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            head.strip_prefix(prefix).is_some_and(|number| {
                matches!(
                    number,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
    {
        return Ok(format!("_{name}"));
    }
    Ok(name)
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

fn equalizer_filters(request: &ProcessMediaRequest) -> Vec<String> {
    let mut filters = Vec::new();

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
    filters
}

fn fade_filters(request: &ProcessMediaRequest, segment_duration: f64) -> Vec<String> {
    let mut filters = Vec::new();
    let max_fade = (segment_duration / 2.0).max(0.0);
    let fade_in = request.fade_in_duration.max(0.0).min(max_fade);
    let fade_out = request.fade_out_duration.max(0.0).min(max_fade);

    if fade_in >= 0.000_5 {
        filters.push(format!("afade=t=in:st=0:d={fade_in:.6}"));
    }

    if fade_out >= 0.000_5 {
        filters.push(format!(
            "afade=t=out:st={:.6}:d={fade_out:.6}",
            segment_duration - fade_out,
        ));
    }

    filters
}

fn apply_output_format_args(
    args: &mut Vec<String>,
    request: &ProcessMediaRequest,
    audio_map: &str,
) {
    let format = request.format.as_str();
    let azuracast_target = is_azuracast_target(request.audio_target.as_deref());
    let normalize = !azuracast_target
        && (request.normalize || request.split_normalization != SplitNormalization::None);
    let default_encoding = EncodingSettings::default();
    let encoding = request.encoding.as_ref().unwrap_or(&default_encoding);

    if is_audio_format(format) {
        args.extend(
            [
                "-map",
                audio_map,
                "-vn",
                "-map_metadata",
                "0",
                "-map_chapters",
                "-1",
            ]
            .iter()
            .map(ToString::to_string),
        );
        if azuracast_target {
            default_encoding.apply_audio_args(args, "mp3", false);
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
        } else {
            encoding.apply_audio_args(args, format, normalize);
        }

        return;
    }

    let video_audio_map = if audio_map == "0:a:0" {
        "0:a:0?"
    } else {
        audio_map
    };
    args.extend(
        [
            "-map",
            "0:v:0?",
            "-map",
            video_audio_map,
            "-map_chapters",
            "-1",
        ]
        .iter()
        .map(ToString::to_string),
    );
    if audio_map != "0:a:0" {
        args.extend(["-map_metadata".into(), "0".into()]);
    }

    encoding.apply_video_args(args, format);
    encoding.apply_audio_args(args, format, normalize);
    if matches!(format, "mp4" | "mov") {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
}

fn is_azuracast_target(target: Option<&str>) -> bool {
    matches!(target, Some("azuracast"))
}

fn parse_ffmpeg_time(value: &str) -> Option<f64> {
    parse_user_time(value)
}

fn parse_user_time(value: &str) -> Option<f64> {
    let parts: Vec<&str> = value.trim().split(':').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut result = 0.0;
    for (index, part) in parts.iter().enumerate() {
        let number: f64 = part.parse().ok()?;
        if !number.is_finite()
            || number < 0.0
            || (index > 0 && number >= 60.0)
            || (index + 1 < parts.len() && number.fract() != 0.0)
        {
            return None;
        }
        result = result * 60.0 + number;
    }
    if result.is_finite() {
        Some(result)
    } else {
        None
    }
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

fn default_true_peak() -> f32 {
    -1.5
}

fn validate_request(request: &ProcessMediaRequest) -> Result<(), String> {
    if !Path::new(&request.input_path).is_file() {
        return Err("El archivo de origen no existe.".into());
    }
    let format = request.format.as_str();
    if !is_audio_format(format) && !is_video_format(format) {
        return Err("Unsupported output format.".into());
    }
    if let Some(encoding) = &request.encoding {
        if is_audio_format(format)
            && is_azuracast_target(request.audio_target.as_deref())
            && encoding != &EncodingSettings::default()
        {
            return Err("AzuraCast uses fixed MP3 encoding settings.".into());
        }
        encoding.validate(format)?;
    }
    if request.split_normalization != SplitNormalization::None
        && (request.normalize || request.split_mode == SplitMode::None)
    {
        return Err("La normalización encadenada requiere un método de división.".into());
    }
    if request
        .split_pause_window_seconds
        .is_some_and(|window| !(0.0..=60.0).contains(&window))
    {
        return Err("La ventana de búsqueda de pausas debe estar entre 0 y 60 s.".into());
    }
    if let Some(pattern) = &request.output_name_pattern {
        split_output_name(pattern, "archivo", 0)?;
    }
    if !request.loudness_target_lufs.is_finite()
        || !(-30.0..=-6.0).contains(&request.loudness_target_lufs)
        || !request.true_peak_dbtp.is_finite()
        || !(-9.0..=0.0).contains(&request.true_peak_dbtp)
        || !request.silence.threshold_db.is_finite()
        || !(-80.0..=-5.0).contains(&request.silence.threshold_db)
        || !request.silence.min_silence_duration.is_finite()
        || !(0.05..=60.0).contains(&request.silence.min_silence_duration)
        || !request.silence.min_segment_duration.is_finite()
        || request.silence.min_segment_duration < 0.001
        || !request.fade_in_duration.is_finite()
        || request.fade_in_duration < 0.0
        || !request.fade_out_duration.is_finite()
        || request.fade_out_duration < 0.0
        || request
            .manual_markers
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0)
        || [request.eq.bass, request.eq.mid, request.eq.treble]
            .iter()
            .any(|v| !v.is_finite() || v.abs() > 24.0)
    {
        return Err("Los parámetros de procesamiento están fuera de rango.".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn preview_segments<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: ProcessMediaRequest,
) -> Result<Vec<Segment>, String> {
    Ok(analyze_split(app, state, request).await?.segments)
}

#[tauri::command]
pub async fn analyze_split<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: ProcessMediaRequest,
) -> Result<SplitPlan, String> {
    validate_request(&request)?;
    let (pid, cancelled) = state.register(&request.id)?;
    let result = async {
        let ffmpeg = resolve_ffmpeg_path(&app)?;
        let info = probe_media_info(&ffmpeg, &request.input_path).await?;
        let (start, end) = resolve_processing_window(
            info.duration_seconds,
            &request.trim_start,
            &request.trim_end,
        )?;
        plan_segments(
            &ffmpeg,
            &request,
            start,
            end,
            waveform_cache_dir(&app).as_deref(),
            &pid,
            &cancelled,
        )
        .await
    }
    .await;
    state.remove(&request.id);
    result
}

fn split_comparison_dir<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<PathBuf, String> {
    if id.len() != 36
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return Err("Identificador de preescucha inválido.".into());
    }
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|err| err.to_string())?
        .join("waveforms")
        .join("split-comparisons")
        .join(id))
}

#[tauri::command]
pub async fn render_split_comparison<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    request: ProcessMediaRequest,
    segment: Segment,
) -> Result<SplitComparison, String> {
    validate_request(&request)?;
    let directory = split_comparison_dir(&app, &request.id)?;
    let (pid, cancelled) = state.register(&request.id)?;
    let result = async {
        let ffmpeg = resolve_ffmpeg_path(&app)?;
        let info = probe_media_info(&ffmpeg, &request.input_path).await?;
        let (start_seconds, end_seconds) = resolve_processing_window(
            info.duration_seconds,
            &request.trim_start,
            &request.trim_end,
        )?;
        if !segment.start_seconds.is_finite()
            || !segment.end_seconds.is_finite()
            || segment.start_seconds < start_seconds - 0.0005
            || segment.end_seconds > end_seconds + 0.0005
            || segment.end_seconds - segment.start_seconds < 0.000999
        {
            return Err("El segmento de preescucha debe estar dentro de la selección.".into());
        }
        let selection = Segment {
            start_seconds,
            end_seconds,
        };
        let parent = directory.parent().unwrap();
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        fs::create_dir(&directory).map_err(|err| err.to_string())?;
        let result = render_comparison_files(
            &ffmpeg,
            request.clone(),
            &selection,
            &segment,
            &directory,
            &pid,
            &cancelled,
        )
        .await;
        if result.is_err() {
            let _ = fs::remove_dir_all(&directory);
        }
        result
    }
    .await;
    state.remove(&request.id);
    result
}

#[tauri::command]
pub fn remove_split_comparison<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
    match fs::remove_dir_all(split_comparison_dir(&app, &id)?) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

async fn render_comparison_files(
    ffmpeg: &Path,
    mut request: ProcessMediaRequest,
    selection: &Segment,
    segment: &Segment,
    directory: &Path,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<SplitComparison, String> {
    request.format = "wav".into();
    request.audio_target = None;
    request.encoding = None;
    let original_path = directory.join("A.wav");
    let processed_path = directory.join("B.wav");
    let normalized_audio = if request.split_normalization == SplitNormalization::Before {
        prepare_normalized_audio(ffmpeg, &request, selection, directory, pid, cancelled).await?
    } else {
        None
    };
    let filter = segment_audio_filter(
        ffmpeg,
        &request,
        segment,
        normalized_audio.is_some(),
        pid,
        cancelled,
    )
    .await?;
    let processed = segment_render_args(
        &request,
        segment,
        &filter,
        &processed_path,
        normalized_audio.as_ref(),
    );
    request.normalize = false;
    request.split_normalization = SplitNormalization::None;
    let original = segment_render_args(&request, segment, "", &original_path, None);
    for args in [original, processed] {
        let mut command = Command::new(ffmpeg);
        command
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let output = tracked_output(&mut command, pid, cancelled).await?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned());
        }
    }
    Ok(SplitComparison {
        original_path: original_path.to_string_lossy().into_owned(),
        processed_path: processed_path.to_string_lossy().into_owned(),
    })
}

fn reserve_staging_path(output: &Path) -> Result<PathBuf, String> {
    let stem = output.file_stem().unwrap_or_default().to_string_lossy();
    let ext = output.extension().unwrap_or_default().to_string_lossy();
    for index in 0..10000 {
        let path = output.with_file_name(format!(".{stem}.render-{index}.{ext}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("No se pudo reservar un archivo temporal.".into())
}

async fn tracked_output(
    command: &mut Command,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<std::process::Output, String> {
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }
    let mut child = command
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Ok(mut guard) = pid.lock() {
        *guard = child.id();
    }
    if cancelled.load(Ordering::SeqCst) {
        let _ = child.kill().await;
    }
    let result = child.wait_with_output().await.map_err(|e| e.to_string());
    if let Ok(mut guard) = pid.lock() {
        *guard = None;
    }
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }
    result
}

fn loudness_filter(request: &ProcessMediaRequest) -> String {
    format!(
        "loudnorm=I={}:TP={}:LRA=11",
        request.loudness_target_lufs, request.true_peak_dbtp
    )
}

fn measured_loudness_filter(
    json: &str,
    target: &str,
    allow_unmeasurable: bool,
) -> Result<Option<String>, String> {
    let start = json.rfind('{').ok_or("No se pudo medir la sonoridad.")?;
    let end = json[start..].find('}').ok_or("Medición incompleta.")? + start;
    let values: serde_json::Value =
        serde_json::from_str(&json[start..=end]).map_err(|e| e.to_string())?;
    if allow_unmeasurable && values["input_i"].as_str() == Some("-inf") {
        return Ok(None);
    }
    let mut result = target.to_string();
    for (key, parameter) in [
        ("input_i", "measured_I"),
        ("input_tp", "measured_TP"),
        ("input_lra", "measured_LRA"),
        ("input_thresh", "measured_thresh"),
        ("target_offset", "offset"),
    ] {
        let number = values[key]
            .as_str()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| v.is_finite())
            .ok_or("No se puede normalizar audio vacío, silencioso o sin sonoridad medible.")?;
        result.push_str(&format!(":{parameter}={number}"));
    }
    result.push_str(":linear=true");
    Ok(Some(result))
}

fn bounded_audio_filter(segment: &Segment, preprocessing: &str, normalizer: &str) -> String {
    let trim = format!(
        "atrim=end={}",
        format_ffmpeg_seconds(segment.end_seconds - segment.start_seconds)
    );
    [trim.as_str(), preprocessing, normalizer]
        .into_iter()
        .filter(|filter| !filter.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

async fn analyze_loudness(
    ffmpeg: &Path,
    request: &ProcessMediaRequest,
    segment: &Segment,
    preprocessing: &str,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Option<String>, String> {
    let target = loudness_filter(request);
    let filter = bounded_audio_filter(
        segment,
        preprocessing,
        &format!("{target}:print_format=json"),
    );
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-ss",
            &format_ffmpeg_seconds(segment.start_seconds),
            "-i",
            &request.input_path,
            "-map",
            "0:a:0",
            "-vn",
            "-af",
            &filter,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = tracked_output(&mut command, pid, cancelled).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    measured_loudness_filter(
        &String::from_utf8_lossy(&output.stderr),
        &target,
        request.split_normalization != SplitNormalization::None,
    )
}

async fn prepare_normalized_audio(
    ffmpeg: &Path,
    request: &ProcessMediaRequest,
    selection: &Segment,
    output_dir: &Path,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<Option<NormalizedAudio>, String> {
    // Keep any initial audio offset so the new track stays aligned with the source video.
    let preprocessing = std::iter::once("aresample=48000:async=1:first_pts=0".into())
        .chain(equalizer_filters(request))
        .collect::<Vec<_>>()
        .join(",");
    let Some(normalizer) =
        analyze_loudness(ffmpeg, request, selection, &preprocessing, pid, cancelled).await?
    else {
        return Ok(None);
    };
    let audio = NormalizedAudio {
        path: reserve_staging_path(&output_dir.join("normalized-audio.wav"))?,
        start_seconds: selection.start_seconds,
    };
    let filter = bounded_audio_filter(selection, &preprocessing, &normalizer);
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "error",
            "-y",
            "-ss",
            &format_ffmpeg_seconds(selection.start_seconds),
            "-i",
            &request.input_path,
            "-map",
            "0:a:0",
            "-vn",
            "-af",
            &filter,
            "-ar",
            "48000",
            "-c:a",
            "pcm_f32le",
            "-rf64",
            "auto",
        ])
        .arg(&audio.path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = tracked_output(&mut command, pid, cancelled).await?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(Some(audio))
}

async fn segment_audio_filter(
    ffmpeg: &Path,
    request: &ProcessMediaRequest,
    segment: &Segment,
    has_normalized_audio: bool,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<String, String> {
    let mut filters = if has_normalized_audio {
        Vec::new()
    } else {
        equalizer_filters(request)
    };
    filters.extend(fade_filters(
        request,
        segment.end_seconds - segment.start_seconds,
    ));
    let preprocessing = filters.join(",");
    if request.normalizes_each_segment() {
        if let Some(normalizer) =
            analyze_loudness(ffmpeg, request, segment, &preprocessing, pid, cancelled).await?
        {
            return Ok(bounded_audio_filter(segment, &preprocessing, &normalizer));
        }
    }
    Ok(preprocessing)
}

fn segment_render_args(
    request: &ProcessMediaRequest,
    segment: &Segment,
    filter_chain: &str,
    output_path: &Path,
    normalized_audio: Option<&NormalizedAudio>,
) -> Vec<String> {
    let segment_duration = segment.end_seconds - segment.start_seconds;
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
    let audio_map = if let Some(audio) = normalized_audio {
        args.extend([
            "-ss".into(),
            format_ffmpeg_seconds(segment.start_seconds - audio.start_seconds),
            "-i".into(),
            audio.path.to_string_lossy().into_owned(),
        ]);
        "1:a:0"
    } else {
        "0:a:0"
    };
    args.push("-t".to_string());
    args.push(format_ffmpeg_seconds(segment_duration));

    if !filter_chain.is_empty() {
        args.push("-af".to_string());
        args.push(filter_chain.to_string());
    }

    apply_output_format_args(&mut args, request, audio_map);

    args.push("-progress".to_string());
    args.push("pipe:1".to_string());
    args.push("-nostats".to_string());
    args.push(output_path.to_string_lossy().to_string());

    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "mediatoolspro-test-{}-{nonce}-{counter}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn request(input: &Path) -> ProcessMediaRequest {
        serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": input, "format": "wav", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 },
            "silence": { "thresholdDb": -40, "minSilenceDuration": 0.5, "minSegmentDuration": 1 }
        }))
        .unwrap()
    }
    fn control() -> (Arc<Mutex<Option<u32>>>, Arc<AtomicBool>) {
        (Arc::new(Mutex::new(None)), Arc::new(AtomicBool::new(false)))
    }
    async fn ffmpeg(args: &[&str]) -> std::process::Output {
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-nostdin", "-y"])
            .args(args)
            .output()
            .await
            .expect("Install FFmpeg to run integration tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
    async fn read_samples(path: &Path) -> Vec<f32> {
        ffmpeg(&["-i", path.to_str().unwrap(), "-f", "f32le", "-"])
            .await
            .stdout
            .chunks_exact(4)
            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
            .collect()
    }

    fn sample_peak(samples: &[f32]) -> f32 {
        samples.iter().map(|value| value.abs()).fold(0.0, f32::max)
    }
    #[tokio::test]
    async fn duration_split_starts_at_the_selection_and_keeps_the_remainder() {
        let preset = |seconds| {
            serde_json::from_value::<ProcessMediaRequest>(serde_json::json!({
                "id": "test", "inputPath": "unused.wav", "format": "wav", "normalize": false,
                "eq": { "bass": 0, "mid": 0, "treble": 0 },
                "splitMode": "duration", "splitDurationSeconds": seconds
            }))
            .expect("fixed-duration split settings should deserialize")
        };
        let (pid, cancelled) = control();
        let request = preset(3.125);
        let segments = build_segments(Path::new("ffmpeg"), &request, 2.0, 9.4, &pid, &cancelled)
            .await
            .unwrap();
        let intervals: Vec<_> = segments
            .iter()
            .map(|segment| (segment.start_seconds, segment.end_seconds))
            .collect();
        assert_eq!(intervals, [(2.0, 5.125), (5.125, 8.25), (8.25, 9.4)]);

        for seconds in [0.0, 0.007, 7.4, 10.0] {
            assert!(
                build_segments(
                    Path::new("ffmpeg"),
                    &preset(seconds),
                    2.0,
                    9.4,
                    &pid,
                    &cancelled
                )
                .await
                .is_err(),
                "Invalid duration or segment count for {seconds} seconds"
            );
        }
        let exact = build_segments(Path::new("ffmpeg"), &request, 2.0, 8.25, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(exact.len(), 2);
        assert_eq!(exact[1].end_seconds, 8.25);
    }
    #[test]
    fn validates_time_inputs_without_silent_fallbacks() {
        assert_eq!(parse_user_time("01:02:03.125"), Some(3723.125));
        assert_eq!(parse_user_time("12.5"), Some(12.5));
        for value in ["NaN", "inf", "-2", "1:60", "1:-5", "1.5:30", "", "1:2:3:4"] {
            assert_eq!(parse_user_time(value), None, "{value}");
        }
        assert!(resolve_processing_window(10.0, &Some("typo".into()), &None).is_err());
        assert!(resolve_processing_window(10.0, &Some("9".into()), &Some("3".into())).is_err());
        assert!(resolve_processing_window(10.0, &None, &Some("11".into())).is_err());
    }
    #[test]
    fn manual_segments_are_sorted_deduplicated_and_contiguous() {
        let segments =
            build_manual_segments(2.0, 10.0, &[8.0, 4.0, 4.0, f64::NAN, 20.0, 0.0]).unwrap();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].start_seconds, 2.0);
        assert_eq!(segments[2].end_seconds, 10.0);
        for pair in segments.windows(2) {
            assert_eq!(pair[0].end_seconds, pair[1].start_seconds);
        }
        assert!(build_manual_segments(0.0, 10.0, &[]).is_err());
        assert_eq!(
            segments_from_boundaries(&[0.0, 0.001, 0.003])
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn split_normalization_order_can_be_saved_with_legacy_requests() {
        let fixture = Fixture::new();
        let input = fixture.0.join("input.wav");
        let legacy = serde_json::json!({
            "id": "test", "inputPath": input, "format": "wav", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 }, "splitMode": "manual"
        });
        for (order, should_resample) in [("none", false), ("before", true), ("after", true)] {
            let mut settings = legacy.clone();
            if order != "none" {
                settings["splitNormalization"] = serde_json::json!(order);
            }
            let request = serde_json::from_value(settings).unwrap();
            let segment = Segment {
                start_seconds: 0.0,
                end_seconds: 4.0,
            };
            let args =
                segment_render_args(&request, &segment, "", &fixture.0.join("out.wav"), None);
            assert_eq!(
                args.windows(2).any(|args| args == ["-ar", "48000"]),
                should_resample,
                "{order}"
            );
        }
    }

    #[test]
    fn normalized_video_export_uses_the_selected_sample_rate_and_bitrate_once() {
        let fixture = Fixture::new();
        let input = fixture.0.join("input.mp4");
        fs::write(&input, "source").unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": input, "format": "mov", "normalize": true,
            "eq": { "bass": 0, "mid": 0, "treble": 0 },
            "encoding": { "sampleRate": 44100, "audioBitrateKbps": 128, "videoQuality": "high" }
        }))
        .unwrap();
        validate_request(&request).unwrap();
        let segment = Segment {
            start_seconds: 0.0,
            end_seconds: 4.0,
        };
        let args = segment_render_args(&request, &segment, "", &fixture.0.join("out.mov"), None);
        let values_for = |option| {
            args.windows(2)
                .filter(|pair| pair[0] == option)
                .map(|pair| pair[1].as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(values_for("-ar"), ["44100"]);
        assert_eq!(values_for("-b:a"), ["128k"]);
        assert_eq!(values_for("-c:v"), ["libx264"]);
        assert_eq!(values_for("-crf"), ["16"]);
    }

    #[test]
    fn rejects_encoding_settings_for_the_wrong_output_format() {
        let fixture = Fixture::new();
        let input = fixture.0.join("input.wav");
        fs::write(&input, "source").unwrap();
        let request = serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": input, "format": "flac", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 },
            "encoding": { "audioBitrateKbps": 128 }
        }))
        .unwrap();
        assert!(validate_request(&request).is_err());
    }

    #[test]
    fn split_exports_expand_the_source_name_and_index_without_overwriting() {
        let fixture = Fixture::new();
        let source = fixture.0.join("Mi concierto.wav");
        fs::write(&source, "source").unwrap();
        let request: ProcessMediaRequest = serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": source, "format": "wav", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 }, "splitMode": "manual",
            "outputNamePattern": "Pista-{numero} - {archivo}"
        }))
        .unwrap();
        let first = build_output_path(&request, &request.input_path, 0, 2).unwrap();
        assert_eq!(first, fixture.0.join("Pista-01 - Mi concierto.wav"));
        fs::write(&first, "previous").unwrap();
        let second = build_output_path(&request, &request.input_path, 0, 2).unwrap();
        assert_eq!(second, fixture.0.join("Pista-01 - Mi concierto_2.wav"));
        assert!(split_output_name("../{numero}", "source", 0).is_err());
        assert!(split_output_name("{archivo}", "source", 0).is_err());
    }

    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn duration_split_finds_a_nearby_pause_and_keeps_unmatched_cuts() {
        let fixture = Fixture::new();
        let input = fixture.0.join("pauses.wav");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            r"aevalsrc=sin(2*PI*997*t)*if(between(t\,3.4\,4.2)\,0\,0.2):s=48000:d=11",
            input.to_str().unwrap(),
        ])
        .await;
        let request: ProcessMediaRequest = serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": input, "format": "wav", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 }, "splitMode": "duration",
            "splitDurationSeconds": 3, "splitPauseWindowSeconds": 1,
            "silence": { "minSilenceDuration": 0.3 }
        }))
        .unwrap();
        let (pid, cancelled) = control();
        let parts = build_segments(Path::new("ffmpeg"), &request, 0.0, 11.0, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(parts.len(), 4);
        assert!((parts[0].end_seconds - 3.8).abs() < 0.01);
        assert_eq!(parts[1].start_seconds, parts[0].end_seconds);
        assert_eq!(parts[1].end_seconds, 6.0);
        assert_eq!(parts[3].end_seconds, 11.0);
    }

    #[test]
    fn exports_never_reuse_existing_names_and_staging_is_unique() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source.wav");
        fs::write(&source, b"original").unwrap();
        let request = request(&source);
        let first = build_output_path(&request, source.to_str().unwrap(), 0, 1).unwrap();
        fs::write(&first, b"previous export").unwrap();
        let second = build_output_path(&request, source.to_str().unwrap(), 0, 1).unwrap();
        assert_ne!(first, second);
        let staging_a = reserve_staging_path(&second).unwrap();
        let staging_b = reserve_staging_path(&second).unwrap();
        assert_ne!(staging_a, staging_b);
        assert_eq!(fs::read(source).unwrap(), b"original");
        assert_eq!(fs::read(first).unwrap(), b"previous export");
    }
    #[test]
    fn simultaneous_exports_publish_distinct_files_without_overwriting() {
        let fixture = Fixture::new();
        let input = fixture.0.join("source.wav");
        fs::write(&input, b"original").unwrap();
        let request = request(&input);
        let output = build_output_path(&request, input.to_str().unwrap(), 0, 1).unwrap();
        let first = reserve_staging_path(&output).unwrap();
        let second = reserve_staging_path(&output).unwrap();
        fs::write(&first, b"first result").unwrap();
        fs::write(&second, b"second result").unwrap();
        let barrier = std::sync::Barrier::new(2);
        let (a, b) = std::thread::scope(|scope| {
            let a = scope.spawn(|| {
                barrier.wait();
                publish_export(&first, &request, 0, 1).unwrap()
            });
            let b = scope.spawn(|| {
                barrier.wait();
                publish_export(&second, &request, 0, 1).unwrap()
            });
            (a.join().unwrap(), b.join().unwrap())
        });
        assert_ne!(a, b);
        assert_eq!(fs::read(a).unwrap(), b"first result");
        assert_eq!(fs::read(b).unwrap(), b"second result");
        assert_eq!(fs::read(input).unwrap(), b"original");
    }

    #[test]
    fn silent_measurements_are_not_treated_as_valid_loudness() {
        let measurement = r#"{"input_i":"-inf","input_tp":"-inf","input_lra":"0","input_thresh":"-70","target_offset":"inf"}"#;
        assert!(
            measured_loudness_filter(measurement, "loudnorm=I=-16:TP=-1.5:LRA=11", false).is_err()
        );
        assert_eq!(
            measured_loudness_filter(measurement, "loudnorm=I=-16:TP=-1.5:LRA=11", true).unwrap(),
            None
        );
    }
    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn automatic_silence_finds_a_pause_above_the_manual_noise_floor() {
        let fixture = Fixture::new();
        let input = fixture.0.join("room-tone.wav");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            r"aevalsrc=sin(2*PI*997*t)*if(between(t\,2.2\,2.9)\,0.045\,0.27):s=48000:d=6",
            input.to_str().unwrap(),
        ])
        .await;
        let mut request = request(&input);
        request.split_mode = SplitMode::Silence;
        request.silence.min_segment_duration = 0.8;
        let (pid, cancelled) = control();
        let engine = Path::new("ffmpeg");
        let manual = build_segments(engine, &request, 0.0, 6.0, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(manual.len(), 1);

        request.silence.automatic = true;
        let plan = plan_segments(engine, &request, 0.0, 6.0, None, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(plan.segments.len(), 2);
        assert!(
            (plan.segments[0].end_seconds - 2.55).abs() < 0.01,
            "{:?}",
            plan.segments
        );
        assert!(plan.automatic.unwrap().threshold_db > -27.0);
        let steady = plan_segments(engine, &request, 3.0, 6.0, None, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(steady.segments.len(), 1);
    }

    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn aiff_export_respects_the_requested_sample_rate_channels_and_bit_depth() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source.wav");
        let output = fixture.0.join("audio.aiff");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=1",
            source.to_str().unwrap(),
        ])
        .await;
        let request: ProcessMediaRequest = serde_json::from_value(serde_json::json!({
            "id": "test", "inputPath": source, "format": "aiff", "normalize": false,
            "eq": { "bass": 0, "mid": 0, "treble": 0 },
            "encoding": { "sampleRate": 96000, "channels": 2, "pcmBitDepth": 24 }
        }))
        .unwrap();
        validate_request(&request).unwrap();
        let segment = Segment {
            start_seconds: 0.0,
            end_seconds: 1.0,
        };
        let args = segment_render_args(&request, &segment, "", &output, None);
        let result = Command::new("ffmpeg").args(args).output().await.unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let decoded = ffmpeg(&["-i", output.to_str().unwrap(), "-f", "null", "-"]).await;
        let info = String::from_utf8_lossy(&decoded.stderr);
        assert!(
            info.contains("pcm_s24be") && info.contains("96000 Hz, stereo"),
            "{info}"
        );
    }

    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn ffmpeg_autosplit_silence_and_manual_export_are_accurate() {
        let fixture = Fixture::new();
        let input = fixture.0.join("pauses.wav");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=44100:cl=mono:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:duration=2",
            "-filter_complex",
            "[0:a][1:a][2:a]concat=n=3:v=0:a=1[out]",
            "-map",
            "[out]",
            input.to_str().unwrap(),
        ])
        .await;
        let (level_pid, level_cancelled) = control();
        let levels = measure_waveform_levels(
            Path::new("ffmpeg"),
            input.to_str().unwrap(),
            &level_pid,
            &level_cancelled,
        )
        .await
        .unwrap();
        assert_eq!(levels.peaks_db.len(), 250);
        assert!((levels.peaks_db[25] + 18.0).abs() < 1.0);
        assert!(levels.peaks_db[125] <= -100.0);
        assert!(levels.rms_db[125] <= -100.0);
        let mut request = request(&input);
        request.split_mode = SplitMode::Silence;
        let (pid, cancelled) = control();
        let segments = build_segments(Path::new("ffmpeg"), &request, 0.0, 5.0, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(segments.len(), 2);
        assert!((segments[0].end_seconds - 2.5).abs() < 0.02);
        let segment = Segment {
            start_seconds: 0.413,
            end_seconds: 1.734,
        };
        let output = fixture.0.join("trim.wav");
        let args = segment_render_args(&request, &segment, "", &output, None);
        let result = Command::new("ffmpeg").args(args).output().await.unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let info = probe_media_info(Path::new("ffmpeg"), output.to_str().unwrap())
            .await
            .unwrap();
        assert!((info.duration_seconds - 1.321).abs() < 0.011);

        let engine = Path::new("ffmpeg");
        request.fade_in_duration = 0.25;
        request.fade_out_duration = 0.25;
        request.manual_markers = vec![1.0];
        for (mode, check_first) in [(SplitMode::Silence, false), (SplitMode::Manual, true)] {
            request.split_mode = mode;
            let parts = build_segments(engine, &request, 0.0, 5.0, &pid, &cancelled)
                .await
                .unwrap();
            let part = if check_first { &parts[0] } else { &parts[1] };
            let filter = segment_audio_filter(engine, &request, part, false, &pid, &cancelled)
                .await
                .unwrap();
            let args = segment_render_args(&request, part, &filter, &output, None);
            let result = Command::new(engine).args(args).output().await.unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let samples = read_samples(&output).await;
            if check_first {
                assert!(sample_peak(&samples[..441]) < 0.006);
            }
            assert!(sample_peak(&samples[samples.len() - 441..]) < 0.006);
            assert!(sample_peak(&samples) > 0.12);
        }
    }
    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn ffmpeg_chapters_keep_short_chapters_and_never_fall_back_to_silence() {
        let fixture = Fixture::new();
        let input = fixture.0.join("chapters.m4a");
        let metadata = fixture.0.join("chapters.txt");
        fs::write(&metadata, ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=2000\ntitle=One\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=2000\nEND=5000\ntitle=Two\n").unwrap();
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=5",
            "-i",
            metadata.to_str().unwrap(),
            "-map_metadata",
            "1",
            "-c:a",
            "aac",
            input.to_str().unwrap(),
        ])
        .await;
        let mut request = request(&input);
        request.split_mode = SplitMode::Chapters;
        request.silence.min_segment_duration = 20.0; // Must not filter embedded chapters.
        let (pid, cancelled) = control();
        let segments = build_segments(Path::new("ffmpeg"), &request, 0.0, 5.0, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(segments.len(), 2);
        assert!((segments[0].end_seconds - 2.0).abs() < 0.001);
        let plain = fixture.0.join("no-chapters.wav");
        ffmpeg(&[
            "-i",
            input.to_str().unwrap(),
            "-map_chapters",
            "-1",
            plain.to_str().unwrap(),
        ])
        .await;
        request.input_path = plain.to_string_lossy().into_owned();
        let error = build_segments(Path::new("ffmpeg"), &request, 0.0, 5.0, &pid, &cancelled)
            .await
            .unwrap_err();
        assert!(error.contains("No hay capítulos"));
    }
    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn ffmpeg_two_pass_normalization_reaches_target_and_peak_limit() {
        let fixture = Fixture::new();
        let input = fixture.0.join("quiet.wav");
        let output = fixture.0.join("normalized.wav");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=1000:duration=6",
            "-af",
            "volume=0.2",
            input.to_str().unwrap(),
        ])
        .await;
        let mut request = request(&input);
        request.normalize = true;
        let segment = Segment {
            start_seconds: 0.0,
            end_seconds: 6.0,
        };
        let (pid, cancelled) = control();
        let filter = analyze_loudness(
            Path::new("ffmpeg"),
            &request,
            &segment,
            "",
            &pid,
            &cancelled,
        )
        .await
        .unwrap()
        .unwrap();
        assert!(filter.contains("measured_I="));
        let args = segment_render_args(&request, &segment, &filter, &output, None);
        let render = Command::new("ffmpeg").args(args).output().await.unwrap();
        assert!(
            render.status.success(),
            "{}",
            String::from_utf8_lossy(&render.stderr)
        );
        let measurement = ffmpeg(&[
            "-i",
            output.to_str().unwrap(),
            "-af",
            "loudnorm=I=-16:TP=-1.5:LRA=11:print_format=json",
            "-f",
            "null",
            "-",
        ])
        .await;
        let raw = String::from_utf8_lossy(&measurement.stderr);
        let start = raw.rfind('{').unwrap();
        let end = raw[start..].find('}').unwrap() + start;
        let values: serde_json::Value = serde_json::from_str(&raw[start..=end]).unwrap();
        let lufs: f64 = values["input_i"].as_str().unwrap().parse().unwrap();
        let peak: f64 = values["input_tp"].as_str().unwrap().parse().unwrap();
        assert!((lufs + 16.0).abs() < 0.2, "LUFS: {lufs}");
        assert!(peak <= -1.4, "True peak: {peak}");
    }

    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn ffmpeg_fades_and_normalizes_fixed_split_in_either_order() {
        let fixture = Fixture::new();
        let input = fixture.0.join("uneven.wav");
        let source =
            r"aevalsrc=sin(2*PI*997*t)*if(lt(t\,1)\,0.9\,if(lt(t\,5)\,0.05\,0.15)):s=48000:d=9";
        ffmpeg(&["-f", "lavfi", "-i", source, input.to_str().unwrap()]).await;

        let mut request = request(&input);
        request.split_mode = SplitMode::Duration;
        request.split_duration_seconds = Some(4.0);
        request.fade_in_duration = 0.25;
        request.fade_out_duration = 0.5;
        let engine = Path::new("ffmpeg");
        let (pid, cancelled) = control();
        let selection = Segment {
            start_seconds: 1.0,
            end_seconds: 9.0,
        };
        let segments = build_segments(engine, &request, 1.0, 9.0, &pid, &cancelled)
            .await
            .unwrap();
        assert_eq!(segments.len(), 2);

        for (order, should_level_parts) in [
            (SplitNormalization::Before, false),
            (SplitNormalization::After, true),
            (SplitNormalization::None, false),
        ] {
            request.split_normalization = order;
            let normalized = if order == SplitNormalization::Before {
                let audio = prepare_normalized_audio(
                    engine, &request, &selection, &fixture.0, &pid, &cancelled,
                )
                .await
                .unwrap()
                .expect("selection should have measurable loudness");
                Some(audio)
            } else {
                None
            };
            let mut peaks = Vec::new();
            for (index, segment) in segments.iter().enumerate() {
                let filter = segment_audio_filter(
                    engine,
                    &request,
                    segment,
                    normalized.is_some(),
                    &pid,
                    &cancelled,
                )
                .await
                .unwrap();
                let output = fixture.0.join(format!("part-{index}.wav"));
                let args =
                    segment_render_args(&request, segment, &filter, &output, normalized.as_ref());
                let rendered = Command::new(engine).args(args).output().await.unwrap();
                assert!(
                    rendered.status.success(),
                    "{}",
                    String::from_utf8_lossy(&rendered.stderr)
                );

                let samples = read_samples(&output).await;
                assert_eq!(samples.len(), 4 * 48_000);
                let middle = sample_peak(&samples[48_000..96_000]);
                assert!(sample_peak(&samples[..480]) < middle * 0.06, "fade in");
                assert!(
                    sample_peak(&samples[samples.len() - 480..]) < middle * 0.04,
                    "fade out"
                );
                peaks.push(middle);
                if order == SplitNormalization::Before {
                    let source_peak = [0.05, 0.15][index];
                    assert!(middle > source_peak * 1.1, "Global gain was not applied");
                }
                if should_level_parts {
                    let measured = ffmpeg(&[
                        "-i",
                        output.to_str().unwrap(),
                        "-af",
                        "loudnorm=print_format=json",
                        "-f",
                        "null",
                        "-",
                    ])
                    .await;
                    let raw = String::from_utf8_lossy(&measured.stderr);
                    let values: serde_json::Value = serde_json::from_str(
                        &raw[raw.rfind('{').unwrap()..=raw.rfind('}').unwrap()],
                    )
                    .unwrap();
                    let lufs = values["input_i"].as_str().unwrap().parse::<f64>().unwrap();
                    assert!((lufs + 16.0).abs() < 0.4, "Part {index}: {lufs} LUFS");
                }
            }
            let ratio = peaks[1] / peaks[0];
            if should_level_parts {
                assert!((0.95..1.05).contains(&ratio), "Peak ratio: {ratio}");
            } else {
                assert!((2.9..3.1).contains(&ratio), "Peak ratio: {ratio}");
            }
            if let Some(audio) = normalized {
                let path = audio.path.clone();
                drop(audio);
                assert!(!path.exists(), "Temporary audio should be removed");
            }
        }
    }

    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn split_comparison_uses_the_same_segment_with_dry_and_processed_audio() {
        let fixture = Fixture::new();
        let source = fixture.0.join("source.wav");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=997:duration=6:sample_rate=48000",
            "-af",
            "volume=0.25",
            source.to_str().unwrap(),
        ])
        .await;
        let mut request = request(&source);
        request.split_mode = SplitMode::Manual;
        request.split_normalization = SplitNormalization::After;
        request.fade_in_duration = 0.3;
        request.fade_out_duration = 0.4;
        let window = Segment {
            start_seconds: 0.5,
            end_seconds: 5.0,
        };
        let segment = Segment {
            start_seconds: 1.25,
            end_seconds: 4.25,
        };
        let (pid, cancelled) = control();
        let comparison = render_comparison_files(
            Path::new("ffmpeg"),
            request.clone(),
            &window,
            &segment,
            &fixture.0,
            &pid,
            &cancelled,
        )
        .await
        .unwrap();
        let original = read_samples(Path::new(&comparison.original_path)).await;
        let processed = read_samples(Path::new(&comparison.processed_path)).await;
        assert_eq!(original.len(), 3 * 48_000);
        assert_eq!(processed.len(), original.len());
        assert!(sample_peak(&original[..480]) > 0.03);
        assert!(sample_peak(&processed[..480]) < 0.012);
        assert!(sample_peak(&processed[48_000..96_000]) > 0.15);
        assert!(sample_peak(&processed[processed.len() - 480..]) < 0.012);

        let filter = segment_audio_filter(
            Path::new("ffmpeg"),
            &request,
            &segment,
            false,
            &pid,
            &cancelled,
        )
        .await
        .unwrap();
        let exported = fixture.0.join("exported.wav");
        let args = segment_render_args(&request, &segment, &filter, &exported, None);
        let result = Command::new("ffmpeg").args(args).output().await.unwrap();
        assert!(result.status.success());
        assert_eq!(processed, read_samples(&exported).await);
    }
    #[cfg(unix)]
    #[tokio::test]
    #[ignore = "Requires FFmpeg on PATH"]
    async fn ffmpeg_tracks_cancellation_and_reaps_the_child() {
        let (pid, cancelled) = control();
        let pid_for_cancel = pid.clone();
        let flag = cancelled.clone();
        let cancel_task = tokio::spawn(async move {
            for _ in 0..100 {
                let child_pid = *pid_for_cancel.lock().unwrap();
                if let Some(child_pid) = child_pid {
                    flag.store(true, Ordering::SeqCst);
                    std::process::Command::new("kill")
                        .args(["-TERM", &child_pid.to_string()])
                        .status()
                        .unwrap();
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            panic!("No child registered");
        });
        let mut command = Command::new("ffmpeg");
        command
            .args([
                "-hide_banner",
                "-nostdin",
                "-re",
                "-f",
                "lavfi",
                "-i",
                "sine=duration=30",
                "-f",
                "null",
                "-",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let result = tracked_output(&mut command, &pid, &cancelled).await;
        cancel_task.await.unwrap();
        assert!(result.unwrap_err().contains("Cancelled"));
        assert!(pid.lock().unwrap().is_none());
    }
}

fn publish_export(
    staging: &Path,
    request: &ProcessMediaRequest,
    index: usize,
    count: usize,
) -> Result<PathBuf, String> {
    for _ in 0..10000 {
        let output = build_output_path(request, &request.input_path, index, count)?;
        match fs::hard_link(staging, &output) {
            Ok(()) => return Ok(output),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("No se pudo publicar la exportación sin sobrescribir archivos.".into())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformLevelsResponse {
    interval_seconds: f64,
    peaks_db: Vec<f32>,
    rms_db: Vec<f32>,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn analyze_waveform<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, JobState>,
    inputPath: String,
    id: String,
) -> Result<WaveformLevelsResponse, String> {
    let (pid, cancelled) = state.register(&id)?;
    let result = async {
        let ffmpeg = resolve_ffmpeg_path(&app)?;
        cached_waveform_levels(
            &ffmpeg,
            &inputPath,
            waveform_cache_dir(&app).as_deref(),
            &pid,
            &cancelled,
        )
        .await
    }
    .await;
    state.remove(&id);
    result
}

fn waveform_cache_dir<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_cache_dir()
        .ok()
        .map(|path| path.join("waveforms"))
}

async fn cached_waveform_levels(
    ffmpeg: &Path,
    input: &str,
    cache: Option<&Path>,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<WaveformLevelsResponse, String> {
    let path = cache.map(|dir| dir.join(format!("levels-v2-{}.json", waveform_cache_key(input))));
    if let Some(levels) = path
        .as_ref()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        return Ok(levels);
    }
    let levels = measure_waveform_levels(ffmpeg, input, pid, cancelled).await?;
    // A failed cache write must not block editing.
    if let Some(path) = path {
        if fs::create_dir_all(path.parent().unwrap()).is_ok() {
            if let Ok(bytes) = serde_json::to_vec(&levels) {
                if let Ok(staging) = reserve_staging_path(&path) {
                    if fs::write(&staging, bytes).is_ok() {
                        let _ = fs::rename(&staging, &path);
                    }
                    let _ = fs::remove_file(&staging);
                }
            }
        }
    }
    Ok(levels)
}

async fn measure_waveform_levels(
    ffmpeg: &Path,
    input: &str,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<WaveformLevelsResponse, String> {
    let mut command = Command::new(ffmpeg);
    command.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i", input, "-map", "0:a:0", "-vn",
        "-af", "aresample=48000:async=1:first_pts=0,asetnsamples=n=960:p=0,astats=metadata=1:reset=1:measure_perchannel=none:measure_overall=Peak_level+RMS_level,ametadata=print:file=-", "-f", "null", "-"])
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }
    let mut child = command
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Ok(mut guard) = pid.lock() {
        *guard = child.id();
    }
    if cancelled.load(Ordering::SeqCst) {
        let _ = child.kill().await;
    }
    let stderr = child
        .stderr
        .take()
        .ok_or("No se pudo leer el análisis de audio.")?;
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut last = String::new();
        while let Ok(Some(line)) = lines.next_line().await {
            last = line;
        }
        last
    });
    let stdout = child
        .stdout
        .take()
        .ok_or("No se pudo leer la forma de onda.")?;
    let mut lines = BufReader::new(stdout).lines();
    let mut peaks_db = Vec::new();
    let mut rms_db = Vec::new();
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if cancelled.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            break;
        }
        let parse_db = |raw: &str| {
            raw.parse::<f32>()
                .ok()
                .filter(|v| v.is_finite())
                .unwrap_or(-120.0)
                .clamp(-120.0, 12.0)
        };
        if let Some(value) = line.strip_prefix("lavfi.astats.Overall.Peak_level=") {
            peaks_db.push(parse_db(value));
        } else if let Some(value) = line.strip_prefix("lavfi.astats.Overall.RMS_level=") {
            rms_db.push(parse_db(value));
        }
    }
    let status = child.wait().await.map_err(|e| e.to_string())?;
    if let Ok(mut guard) = pid.lock() {
        *guard = None;
    }
    let message = stderr_task.await.map_err(|e| e.to_string())?;
    if cancelled.load(Ordering::SeqCst) {
        return Err("Cancelled by user".into());
    }
    if !status.success() || peaks_db.is_empty() || peaks_db.len() != rms_db.len() {
        return Err(if message.is_empty() {
            "No se pudo medir el nivel del audio.".into()
        } else {
            message
        });
    }
    Ok(WaveformLevelsResponse {
        interval_seconds: 0.02,
        peaks_db,
        rms_db,
    })
}
