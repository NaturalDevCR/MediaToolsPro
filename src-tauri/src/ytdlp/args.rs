use crate::ytdlp::DownloadRequest;

pub fn is_audio_format(format: &str) -> bool {
    matches!(format, "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus")
}

fn audio_format_for_ytdlp(format: &str) -> &str {
    match format {
        "ogg" => "vorbis",
        other => other,
    }
}

pub fn is_youtube_url(url: &str) -> bool {
    url.contains("youtube.com/") || url.contains("youtu.be/")
}

fn is_youtube_radio_mix_url(url: &str) -> bool {
    is_youtube_url(url) && (url.contains("start_radio=1") || url.contains("list=RD"))
}

pub fn build_download_args(
    req: &DownloadRequest,
    ffmpeg_dir: &str,
    client: Option<&str>,
    has_cookies: bool,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-warnings".into(),
        "--ffmpeg-location".into(),
        ffmpeg_dir.into(),
        "-P".into(),
        req.output_path.clone(),
        "--progress".into(),
        "--print".into(),
        "after_move:filepath".into(),
    ];

    if let Some(tpl) = req
        .output_template
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push("-o".into());
        args.push(tpl.to_string());
    }

    if is_youtube_url(&req.url) && !has_cookies {
        if let Some(c) = client {
            args.push("--extractor-args".into());
            args.push(format!("youtube:player_client={}", c));
        }
    }

    if has_cookies {
        if let Some(cookies) = req
            .cookies_file
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            args.push("--cookies".into());
            args.push(cookies.to_string());
        }
    }

    match req.playlist_mode.as_deref() {
        Some("playlist") => args.push("--yes-playlist".into()),
        Some("single") => args.push("--no-playlist".into()),
        _ if is_youtube_radio_mix_url(&req.url) => args.push("--no-playlist".into()),
        _ if req.url.contains("list=") => args.push("--yes-playlist".into()),
        _ => args.push("--no-playlist".into()),
    }

    if let Some(items) = req
        .playlist_items
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        args.push("--playlist-items".into());
        args.push(items.to_string());
    }

    let is_audio = is_audio_format(&req.format);
    let recode = req.recode.unwrap_or(false);

    if let Some(format_id) = req.format_id.as_deref().filter(|s| !s.is_empty()) {
        // A video-only stream picked from the explorer has no audio; mux in best
        // audio (falling back to the bare stream if it can't be combined).
        let needs_audio = !is_audio && req.format_has_audio == Some(false);
        args.push("-f".into());
        if needs_audio {
            args.push(format!("{id}+ba/{id}", id = format_id));
        } else {
            args.push(format_id.to_string());
        }
        if is_audio {
            push_audio_extract(&mut args, &req.format, &req.quality);
        } else if recode {
            args.push("--recode-video".into());
            args.push(req.format.clone());
        } else {
            if needs_audio {
                args.push("--merge-output-format".into());
                args.push(req.format.clone());
            }
            args.push("--remux-video".into());
            args.push(req.format.clone());
        }
    } else if is_audio {
        push_audio_extract(&mut args, &req.format, &req.quality);
    } else {
        args.push("-f".into());
        if req.quality != "best" {
            args.push(format!(
                "bv*[height<={h}]+ba/b[height<={h}]/b",
                h = req.quality
            ));
        } else {
            args.push("bv*+ba/b".into());
        }
        if recode {
            args.push("--recode-video".into());
            args.push(req.format.clone());
        } else {
            args.push("--merge-output-format".into());
            args.push(req.format.clone());
            args.push("--remux-video".into());
            args.push(req.format.clone());
        }
    }

    push_postprocessing(&mut args, req, is_audio);

    args.push(req.url.clone());
    args
}

fn push_audio_extract(args: &mut Vec<String>, format: &str, quality: &str) {
    args.push("-x".into());
    args.push("--audio-format".into());
    args.push(audio_format_for_ytdlp(format).into());
    args.push("--audio-quality".into());
    if quality != "best" {
        args.push(format!("{}K", quality));
    } else {
        args.push("0".into());
    }
}

fn push_postprocessing(args: &mut Vec<String>, req: &DownloadRequest, is_audio: bool) {
    if req.embed_thumbnail.unwrap_or(false) {
        args.push("--embed-thumbnail".into());
    }
    if req.embed_metadata.unwrap_or(false) {
        args.push("--embed-metadata".into());
    }
    if req.embed_chapters.unwrap_or(false) {
        args.push("--embed-chapters".into());
    }
    if !is_audio && req.embed_subs.unwrap_or(false) {
        args.push("--embed-subs".into());
        let langs = req
            .sub_langs
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("en.*");
        args.push("--sub-langs".into());
        args.push(langs.to_string());
    }
    match req.sponsorblock.as_deref() {
        Some("mark") => {
            args.push("--sponsorblock-mark".into());
            args.push("all".into());
        }
        Some("remove") => {
            args.push("--sponsorblock-remove".into());
            args.push("default".into());
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ytdlp::DownloadRequest;

    fn base_req() -> DownloadRequest {
        DownloadRequest {
            id: "t".into(),
            url: "https://youtu.be/abc".into(),
            format: "mp4".into(),
            quality: "1080".into(),
            format_id: None,
            format_has_audio: None,
            output_path: "/out".into(),
            playlist_mode: Some("single".into()),
            audio_target: None,
            video_target: None,
            cookies_file: None,
            recode: None,
            embed_thumbnail: None,
            embed_metadata: None,
            embed_chapters: None,
            embed_subs: None,
            sub_langs: None,
            sponsorblock: None,
            playlist_items: None,
            output_template: None,
        }
    }

    fn pair(args: &[String], a: &str, b: &str) -> bool {
        args.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn video_default_remuxes_not_recodes() {
        let args = build_download_args(&base_req(), "/ff", Some("tv"), false);
        assert!(pair(&args, "--merge-output-format", "mp4"));
        assert!(pair(&args, "--remux-video", "mp4"));
        assert!(!args.iter().any(|a| a == "--recode-video"));
        assert!(pair(&args, "-f", "bv*[height<=1080]+ba/b[height<=1080]/b"));
    }

    #[test]
    fn video_only_format_id_muxes_in_audio() {
        let mut req = base_req();
        req.format_id = Some("137".into());
        req.format_has_audio = Some(false);
        let args = build_download_args(&req, "/ff", Some("tv"), false);
        assert!(pair(&args, "-f", "137+ba/137"));
        assert!(pair(&args, "--merge-output-format", "mp4"));
        assert!(pair(&args, "--remux-video", "mp4"));
    }

    #[test]
    fn combined_format_id_stays_single_stream() {
        let mut req = base_req();
        req.format_id = Some("18".into());
        req.format_has_audio = Some(true);
        let args = build_download_args(&req, "/ff", Some("tv"), false);
        assert!(pair(&args, "-f", "18"));
        assert!(!args.iter().any(|a| a == "--merge-output-format"));
    }

    #[test]
    fn video_recode_when_forced() {
        let mut req = base_req();
        req.recode = Some(true);
        let args = build_download_args(&req, "/ff", Some("tv"), false);
        assert!(pair(&args, "--recode-video", "mp4"));
        assert!(!args.iter().any(|a| a == "--remux-video"));
    }

    #[test]
    fn audio_extracts_with_quality() {
        let mut req = base_req();
        req.format = "mp3".into();
        req.quality = "192".into();
        let args = build_download_args(&req, "/ff", None, false);
        assert!(args.iter().any(|a| a == "-x"));
        assert!(pair(&args, "--audio-format", "mp3"));
        assert!(pair(&args, "--audio-quality", "192K"));
    }

    #[test]
    fn youtube_client_only_without_cookies() {
        let with = build_download_args(&base_req(), "/ff", Some("tv"), false);
        assert!(pair(&with, "--extractor-args", "youtube:player_client=tv"));
        let without = build_download_args(&base_req(), "/ff", Some("tv"), true);
        assert!(!without.iter().any(|a| a == "--extractor-args"));
    }

    #[test]
    fn embed_and_sponsorblock_flags() {
        let mut req = base_req();
        req.embed_thumbnail = Some(true);
        req.sponsorblock = Some("remove".into());
        let args = build_download_args(&req, "/ff", Some("tv"), false);
        assert!(args.iter().any(|a| a == "--embed-thumbnail"));
        assert!(pair(&args, "--sponsorblock-remove", "default"));
    }

    #[test]
    fn playlist_items_range() {
        let mut req = base_req();
        req.playlist_mode = Some("playlist".into());
        req.playlist_items = Some("1-5".into());
        let args = build_download_args(&req, "/ff", Some("tv"), false);
        assert!(args.iter().any(|a| a == "--yes-playlist"));
        assert!(pair(&args, "--playlist-items", "1-5"));
    }
}
