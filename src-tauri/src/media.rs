use crate::jobs::{
    emit_job_progress, emit_log, is_audio_format, media_kind_for_format, JobProgressPayload,
    JobState,
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
    #[serde(default = "default_true_peak")]
    pub true_peak_dbtp: f32,
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

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
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
    let segments = build_segments(
        &ffmpeg_path,
        &request,
        region_start,
        region_end,
        &pid,
        &cancelled,
    )
    .await?;
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
    let mut filter_chain = build_filter_chain(request, segment_duration);
    if request.normalize {
        emit_job_progress(
            app,
            JobProgressPayload {
                id: request.id.clone(),
                job_kind: "process".into(),
                media_kind: media_kind_for_format(&request.format).into(),
                status: "processing".into(),
                percent: segment_index as f64 / segment_count as f64 * 100.0,
                speed: "-".into(),
                eta: "-".into(),
                total_size: format!("{segment_count} segmento(s)"),
                title: Some(file_name.into()),
                detail: Some(format!(
                    "Midiendo sonoridad · segmento {}/{} · pasada 1/2",
                    segment_index + 1,
                    segment_count
                )),
                output_path: None,
                error: None,
            },
        );
        let normalizer = analyze_loudness(
            ffmpeg_path,
            request,
            segment,
            &filter_chain,
            &pid,
            &cancelled,
        )
        .await?;
        if !filter_chain.is_empty() {
            filter_chain.push(',');
        }
        filter_chain.push_str(&normalizer);
    }

    let args = segment_render_args(request, segment, &filter_chain, output_path);

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
    markers.dedup_by(|left, right| (*left - *right).abs() < 0.001);

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
    let mut command = Command::new(ffmpeg_path);
    command
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
        .stderr(Stdio::piped());
    let output = tracked_output(&mut command, pid, cancelled).await?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
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

    let file_name = if segment_count > 1 {
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
        args.extend(
            [
                "-map",
                "0:a:0",
                "-vn",
                "-map_metadata",
                "0",
                "-map_chapters",
                "-1",
            ]
            .iter()
            .map(ToString::to_string),
        );
        let azuracast_target = is_azuracast_target(audio_target);

        match if azuracast_target { "mp3" } else { format } {
            "mp3" => {
                args.extend(
                    ["-c:a", "libmp3lame", "-b:a", "320k"]
                        .iter()
                        .map(ToString::to_string),
                );
            }
            "opus" => {
                args.extend(
                    ["-c:a", "libopus", "-b:a", "192k"]
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
        ["-map", "0:v:0?", "-map", "0:a:0?", "-map_chapters", "-1"]
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
    if !matches!(
        request.format.as_str(),
        "wav" | "flac" | "mp3" | "m4a" | "aac" | "ogg" | "opus" | "mp4" | "mkv" | "webm"
    ) {
        return Err("Formato de salida no compatible.".into());
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
        build_segments(&ffmpeg, &request, start, end, &pid, &cancelled).await
    }
    .await;
    state.remove(&request.id);
    result
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

fn measured_loudness_filter(json: &str, target: &str) -> Result<String, String> {
    let start = json.rfind('{').ok_or("No se pudo medir la sonoridad.")?;
    let end = json[start..].find('}').ok_or("Medición incompleta.")? + start;
    let values: serde_json::Value =
        serde_json::from_str(&json[start..=end]).map_err(|e| e.to_string())?;
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
    Ok(result)
}

async fn analyze_loudness(
    ffmpeg: &Path,
    request: &ProcessMediaRequest,
    segment: &Segment,
    preprocessing: &str,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<String, String> {
    let target = loudness_filter(request);
    let filter = if preprocessing.is_empty() {
        format!("{target}:print_format=json")
    } else {
        format!("{preprocessing},{target}:print_format=json")
    };
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-ss",
            &format_ffmpeg_seconds(segment.start_seconds),
            "-i",
            &request.input_path,
            "-t",
            &format_ffmpeg_seconds(segment.end_seconds - segment.start_seconds),
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
    measured_loudness_filter(&String::from_utf8_lossy(&output.stderr), &target)
}

fn segment_render_args(
    request: &ProcessMediaRequest,
    segment: &Segment,
    filter_chain: &str,
    output_path: &Path,
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
    args.push("-t".to_string());
    args.push(format_ffmpeg_seconds(segment_duration));

    if !filter_chain.is_empty() {
        args.push("-af".to_string());
        args.push(filter_chain.to_string());
    }

    apply_output_format_args(&mut args, &request.format, request.audio_target.as_deref());
    if request.normalize && !is_azuracast_target(request.audio_target.as_deref()) {
        args.extend(["-ar".into(), "48000".into()]);
    }

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
        assert!(measured_loudness_filter(measurement, "loudnorm=I=-16:TP=-1.5:LRA=11").is_err());
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
        let args = segment_render_args(&request, &segment, "", &output);
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
        .unwrap();
        assert!(filter.contains("measured_I="));
        let args = segment_render_args(&request, &segment, &filter, &output);
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
        let cache = app
            .path()
            .app_cache_dir()
            .map_err(|e| e.to_string())?
            .join("waveforms");
        fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
        let path = cache.join(format!("levels-v1-{}.json", waveform_cache_key(&inputPath)));
        if let Ok(bytes) = fs::read(&path) {
            if let Ok(levels) = serde_json::from_slice::<WaveformLevelsResponse>(&bytes) {
                return Ok(levels);
            }
        }
        let ffmpeg = resolve_ffmpeg_path(&app)?;
        let levels = measure_waveform_levels(&ffmpeg, &inputPath, &pid, &cancelled).await?;
        // Cache write is optional. A failed cache write must not block editing.
        if let Ok(bytes) = serde_json::to_vec(&levels) {
            if let Ok(staging) = reserve_staging_path(&path) {
                if fs::write(&staging, bytes).is_ok() {
                    let _ = fs::rename(&staging, &path);
                }
                let _ = fs::remove_file(&staging);
            }
        }
        Ok(levels)
    }
    .await;
    state.remove(&id);
    result
}

async fn measure_waveform_levels(
    ffmpeg: &Path,
    input: &str,
    pid: &Arc<Mutex<Option<u32>>>,
    cancelled: &Arc<AtomicBool>,
) -> Result<WaveformLevelsResponse, String> {
    let mut command = Command::new(ffmpeg);
    command.args(["-hide_banner", "-nostdin", "-loglevel", "error", "-i", input, "-map", "0:a:0", "-vn",
        "-af", "aresample=48000,asetnsamples=n=960:p=0,astats=metadata=1:reset=1:measure_perchannel=none:measure_overall=Peak_level+RMS_level,ametadata=print:file=-", "-f", "null", "-"])
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
