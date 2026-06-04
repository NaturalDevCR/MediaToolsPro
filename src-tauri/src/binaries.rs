use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter, Manager, Runtime};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryStatus {
    pub ytdlp: bool,
    pub ytdlp_version: Option<String>,
    pub ffmpeg: bool,
    pub ffmpeg_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryVersions {
    pub ytdlp_version: Option<String>,
    pub ffmpeg_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BinaryUpdateStatus {
    pub ytdlp_update_available: bool,
    pub ytdlp_latest_version: Option<String>,
    pub ffmpeg_update_available: bool,
    pub ffmpeg_latest_version: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinaryProgress {
    pub binary: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct VersionCache {
    ytdlp_version: Option<String>,
    ffmpeg_version: Option<String>,
}

fn get_bin_dir<R: Runtime>(app: &AppHandle<R>) -> PathBuf {
    let path = app
        .path()
        .app_data_dir()
        .expect("failed to get app data dir")
        .join("bin");
    if !path.exists() {
        fs::create_dir_all(&path).expect("failed to create bin dir");
    }
    path
}

fn get_ytdlp_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    }
}

fn get_ffmpeg_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}

pub fn part_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(".part");
    PathBuf::from(value)
}

pub fn ytdlp_download_url(os: &str, channel: &str) -> &'static str {
    match (os, channel) {
        ("windows", "nightly") => {
            "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe"
        }
        ("macos", "nightly") => {
            "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_macos"
        }
        (_, "nightly") => {
            "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp"
        }
        ("windows", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("macos", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        (_, _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
    }
}

fn current_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn cache_path(bin_dir: &PathBuf) -> PathBuf {
    bin_dir.join(".versions.json")
}

fn read_cache(bin_dir: &PathBuf) -> VersionCache {
    let path = cache_path(bin_dir);
    if let Ok(data) = fs::read_to_string(&path) {
        serde_json::from_str(&data).unwrap_or_default()
    } else {
        VersionCache::default()
    }
}

fn write_cache(bin_dir: &PathBuf, cache: &VersionCache) {
    let path = cache_path(bin_dir);
    if let Ok(json) = serde_json::to_string(cache) {
        let _ = fs::write(&path, json);
    }
}

fn run_ytdlp_version(binary: &PathBuf) -> Option<String> {
    Command::new(binary)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

fn run_ffmpeg_version(binary: &PathBuf) -> Option<String> {
    Command::new(binary)
        .arg("-version")
        .output()
        .ok()
        .and_then(|o| {
            let out = String::from_utf8_lossy(&o.stdout);
            out.lines().next().map(|s| s.to_string())
        })
}

fn cache_single(bin_dir: &PathBuf, name: &str) {
    let binary = bin_dir.join(name);
    if !binary.exists() {
        return;
    }
    let mut cache = read_cache(bin_dir);
    if name == get_ytdlp_name() {
        cache.ytdlp_version = run_ytdlp_version(&binary);
    } else if name == get_ffmpeg_name() {
        cache.ffmpeg_version = run_ffmpeg_version(&binary);
    }
    write_cache(bin_dir, &cache);
}

fn clear_cached(bin_dir: &PathBuf, name: &str) {
    let mut cache = read_cache(bin_dir);
    if name == get_ytdlp_name() {
        cache.ytdlp_version = None;
    } else if name == get_ffmpeg_name() {
        cache.ffmpeg_version = None;
    }
    write_cache(bin_dir, &cache);
}

fn parse_numeric_version(s: &str) -> Option<Vec<u32>> {
    let re = regex::Regex::new(r"(\d+\.\d+(?:\.\d+)?)").ok()?;
    let caps = re.captures(s)?;
    let version_str = caps.get(1)?.as_str();
    let parts: Option<Vec<u32>> = version_str.split('.').map(|p| p.parse::<u32>().ok()).collect();
    if parts.as_ref().map_or(true, |v| v.is_empty()) {
        return None;
    }
    parts
}

fn compare_versions(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    for (va, vb) in a.iter().zip(b.iter()) {
        match va.cmp(vb) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    a.len().cmp(&b.len())
}

async fn fetch_ytdlp_latest_version() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .ok()?;
    let resp = client
        .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
        .header("User-Agent", "MediaToolsPro")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    let json: serde_json::Value = resp.json().await.ok()?;
    json["tag_name"].as_str().map(|s| s.to_string())
}

#[tauri::command]
pub async fn check_binaries<R: Runtime>(app: AppHandle<R>) -> BinaryStatus {
    let bin_dir = get_bin_dir(&app);
    let ytdlp_path = bin_dir.join(get_ytdlp_name());
    let ffmpeg_path = bin_dir.join(get_ffmpeg_name());
    let cache = read_cache(&bin_dir);

    let ytdlp_exists = ytdlp_path.exists();
    let ffmpeg_exists = ffmpeg_path.exists();

    let needs_ytdlp = ytdlp_exists && cache.ytdlp_version.is_none();
    let needs_ffmpeg = ffmpeg_exists && cache.ffmpeg_version.is_none();

    if needs_ytdlp {
        let bin_dir_clone = bin_dir.clone();
        tokio::spawn(async move {
            let binary = bin_dir_clone.join(get_ytdlp_name());
            if let Some(version) = run_ytdlp_version(&binary) {
                let mut c = read_cache(&bin_dir_clone);
                c.ytdlp_version = Some(version);
                write_cache(&bin_dir_clone, &c);
            }
        });
    }

    if needs_ffmpeg {
        let bin_dir_clone = bin_dir.clone();
        tokio::spawn(async move {
            let binary = bin_dir_clone.join(get_ffmpeg_name());
            if let Some(version) = run_ffmpeg_version(&binary) {
                let mut c = read_cache(&bin_dir_clone);
                c.ffmpeg_version = Some(version);
                write_cache(&bin_dir_clone, &c);
            }
        });
    }

    BinaryStatus {
        ytdlp: ytdlp_exists,
        ytdlp_version: cache.ytdlp_version.clone(),
        ffmpeg: ffmpeg_exists,
        ffmpeg_version: cache.ffmpeg_version.clone(),
    }
}

#[tauri::command]
pub async fn get_binary_versions<R: Runtime>(app: AppHandle<R>) -> BinaryVersions {
    let bin_dir = get_bin_dir(&app);
    let cache = read_cache(&bin_dir);
    BinaryVersions {
        ytdlp_version: cache.ytdlp_version,
        ffmpeg_version: cache.ffmpeg_version,
    }
}

#[tauri::command]
pub async fn check_binary_updates<R: Runtime>(app: AppHandle<R>) -> Result<BinaryUpdateStatus, String> {
    let bin_dir = get_bin_dir(&app);
    let cache = read_cache(&bin_dir);

    let (ytdlp_update, ytdlp_latest) = if let Some(ref current) = cache.ytdlp_version {
        if let Some(latest) = fetch_ytdlp_latest_version().await {
            let is_newer = match (parse_numeric_version(current), parse_numeric_version(&latest))
            {
                (Some(c), Some(l)) => compare_versions(&l, &c) == std::cmp::Ordering::Greater,
                _ => current != &latest,
            };
            (is_newer, Some(latest))
        } else {
            (false, None)
        }
    } else {
        (false, None)
    };

    Ok(BinaryUpdateStatus {
        ytdlp_update_available: ytdlp_update,
        ytdlp_latest_version: ytdlp_latest,
        ffmpeg_update_available: false,
        ffmpeg_latest_version: None,
    })
}

#[tauri::command]
pub async fn install_ytdlp<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ytdlp_name());

    let url = ytdlp_download_url(current_os(), "stable");
    download_file(&app, url, &target, "yt-dlp").await?;

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&target)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms).map_err(|e| e.to_string())?;
    }

    cache_single(&bin_dir, get_ytdlp_name());

    Ok(())
}

#[tauri::command]
pub async fn install_ffmpeg<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let ffmpeg_target = bin_dir.join(get_ffmpeg_name());

    let (url, is_zip) = if cfg!(target_os = "macos") {
        ("https://evermeet.cx/ffmpeg/getrelease/zip", true)
    } else if cfg!(target_os = "windows") {
        (
            "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
            true,
        )
    } else {
        (
            "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz",
            false,
        )
    };

    if is_zip {
        let tmp_zip = bin_dir.join("ffmpeg.zip");
        download_file(&app, url, &tmp_zip, "ffmpeg").await?;

        let file = fs::File::open(&tmp_zip).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
            let name = file.name();
            if name.ends_with("ffmpeg") || name.ends_with("ffmpeg.exe") {
                let mut outfile = fs::File::create(&ffmpeg_target).map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
                break;
            }
        }

        let _ = fs::remove_file(tmp_zip);

        #[cfg(unix)]
        {
            if ffmpeg_target.exists() {
                let mut perms = fs::metadata(&ffmpeg_target)
                    .map_err(|e| e.to_string())?
                    .permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&ffmpeg_target, perms).map_err(|e| e.to_string())?;
            }
        }
    }

    cache_single(&bin_dir, get_ffmpeg_name());

    Ok(())
}

async fn download_file<R: Runtime>(
    app: &AppHandle<R>,
    url: &str,
    path: &PathBuf,
    label: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .header("User-Agent", "MediaToolsPro")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed for {} (HTTP {})",
            label,
            response.status().as_u16()
        ));
    }

    let total = response.content_length();
    let part = part_path(path);
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut downloaded = 0_u64;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = total
            .map(|t| (downloaded as f64 / t as f64) * 100.0)
            .unwrap_or(0.0);
        let _ = app.emit(
            "binary-progress",
            BinaryProgress {
                binary: label.to_string(),
                downloaded,
                total,
                percent,
            },
        );
    }

    drop(file);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    fs::rename(&part, path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn delete_ytdlp<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ytdlp_name());
    if target.exists() {
        fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    clear_cached(&bin_dir, get_ytdlp_name());
    Ok(())
}

#[tauri::command]
pub async fn delete_ffmpeg<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let bin_dir = get_bin_dir(&app);
    let target = bin_dir.join(get_ffmpeg_name());
    if target.exists() {
        fs::remove_file(&target).map_err(|e| e.to_string())?;
    }
    clear_cached(&bin_dir, get_ffmpeg_name());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn part_path_appends_suffix() {
        let p = part_path(Path::new("/bin/yt-dlp"));
        assert_eq!(p.to_string_lossy(), "/bin/yt-dlp.part");
    }

    #[test]
    fn url_selects_channel_and_os() {
        assert!(ytdlp_download_url("macos", "stable").contains("yt-dlp/releases"));
        assert!(ytdlp_download_url("macos", "nightly").contains("yt-dlp-nightly-builds"));
        assert!(ytdlp_download_url("windows", "stable").ends_with("yt-dlp.exe"));
    }
}
