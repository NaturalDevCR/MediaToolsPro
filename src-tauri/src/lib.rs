use std::path::Path;
use std::process::Command;

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    let reveal_target = if target.is_dir() {
        target
    } else {
        target.parent().unwrap_or(target)
    };

    #[cfg(target_os = "macos")]
    let status = if target.exists() && !target.is_dir() {
        Command::new("open")
            .args(["-R", &target.to_string_lossy()])
            .status()
    } else {
        Command::new("open").arg(reveal_target).status()
    };

    #[cfg(target_os = "windows")]
    let status = if target.exists() && !target.is_dir() {
        Command::new("explorer")
            .arg(format!("/select,{}", target.to_string_lossy()))
            .status()
    } else {
        Command::new("explorer").arg(reveal_target).status()
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let status = Command::new("xdg-open").arg(reveal_target).status();

    status
        .map_err(|error| error.to_string())
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err("The file browser could not be opened.".into())
            }
        })
}

mod binaries;
mod jobs;
mod media;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(jobs::JobState::new())
        .invoke_handler(tauri::generate_handler![
            reveal_path,
            binaries::check_binaries,
            binaries::install_ffmpeg,
            jobs::cancel_job,
            media::probe_media,
            media::analyze_waveform,
            media::preview_segments,
            media::render_waveform_preview,
            media::start_media_process
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
