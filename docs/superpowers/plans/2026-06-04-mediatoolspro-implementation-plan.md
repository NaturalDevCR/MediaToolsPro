# MediaToolsPro Audit & Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make YouTube downloads work reliably like MeTube and keep working over time, harden the whole app, redesign the UI to be professional and intuitive, and decompose the 3,589-line frontend god-component — delivered in four independently-shippable phases.

**Architecture:** Phase 1 refactors the Rust download path into small, pure, unit-tested modules (argument building, client fallback, error normalization, progress parsing) and hardens binary management (streamed installs, yt-dlp auto-update). Phases 2–4 rebuild the Vue frontend: a behavior-preserving decomposition into views/composables/components with a design-token system, then a sidebar-based redesign with first-run onboarding and the new download options, then robustness polish.

**Tech Stack:** Rust (Tauri 2, tokio, reqwest, regex, serde), Vue 3 `<script setup>` + TypeScript, Tailwind v4, Vite, Vitest (added in Phase 2), pnpm.

**Companion spec:** `docs/superpowers/specs/2026-06-04-mediatoolspro-audit-redesign-design.md`

---

## Plan structure & how to use it

- **Phase 1 is fully detailed** below (bite-sized TDD tasks with complete code). It is the user's top priority and ships "downloads like MeTube" before any UI change.
- **Phases 2–4 are a task-level roadmap.** Each is expanded into full bite-sized steps *immediately before it is executed* (re-run `superpowers:writing-plans` for that phase), because the exact UI step content depends on the structure that emerges from the Phase 2 refactor. This avoids speculative code that drifts.
- Every phase leaves `pnpm tauri dev` working and `cargo test` green.

### Verification commands (run from repo root unless noted)
- Rust tests: `cd src-tauri && cargo test`
- Rust build: `cd src-tauri && cargo build`
- Frontend type-check: `pnpm build` (runs `vue-tsc --noEmit && vite build`)
- Frontend unit tests (after Phase 2 Task 1): `pnpm test`
- Full app: `pnpm tauri dev`

---

# PHASE 1 — Download engine + binaries (backend, testable)

**Outcome:** remux-by-default downloads, a YouTube player-client fallback chain, embed/SponsorBlock/playlist options, friendly errors, streamed+validated binary installs, and yt-dlp auto-update — all covered by `cargo test`. No frontend changes required for Phase 1 to function (new request fields are optional/back-compatible).

## Task 1: Convert `ytdlp.rs` into a module directory

**Files:**
- Move: `src-tauri/src/ytdlp.rs` → `src-tauri/src/ytdlp/mod.rs`

- [ ] **Step 1: Move the file with git**

```bash
cd src-tauri/src
mkdir -p ytdlp
git mv ytdlp.rs ytdlp/mod.rs
cd ../..
```

- [ ] **Step 2: Declare submodules at the top of `ytdlp/mod.rs`**

Add these lines directly under the existing `use` block at the top of `src-tauri/src/ytdlp/mod.rs`:

```rust
mod args;
mod clients;
mod errors;
mod progress;
```

- [ ] **Step 3: Make `DownloadRequest` visible to submodules**

`DownloadRequest` is already `pub struct` in `mod.rs`; submodules will reference it via `use super::DownloadRequest;`. No change needed beyond confirming it is `pub`.

- [ ] **Step 4: Create empty submodule files so it compiles**

```bash
cd src-tauri/src/ytdlp
printf '' > args.rs && printf '' > clients.rs && printf '' > errors.rs && printf '' > progress.rs
cd ../../..
```

- [ ] **Step 5: Verify build**

Run: `cd src-tauri && cargo build`
Expected: builds (warnings about unused modules are fine).

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "refactor(ytdlp): split into module directory"
```

## Task 2: Extend `DownloadRequest` with optional download options

**Files:**
- Modify: `src-tauri/src/ytdlp/mod.rs` (the `DownloadRequest` struct, ~line 17-30)

- [ ] **Step 1: Write the failing test**

Add to the bottom of `src-tauri/src/ytdlp/mod.rs`:

```rust
#[cfg(test)]
mod request_tests {
    use super::DownloadRequest;

    #[test]
    fn deserializes_legacy_payload_with_defaults() {
        // Old frontend payload without the new fields must still parse.
        let json = r#"{
            "id":"x","url":"https://youtu.be/a","format":"mp4","quality":"best",
            "outputPath":"/out"
        }"#;
        let req: DownloadRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.recode, None);
        assert_eq!(req.embed_thumbnail, None);
        assert_eq!(req.sponsorblock, None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test request_tests`
Expected: FAIL to compile — fields `recode`, `embed_thumbnail`, `sponsorblock` do not exist.

- [ ] **Step 3: Add the fields**

In `src-tauri/src/ytdlp/mod.rs`, extend the `DownloadRequest` struct (serde maps missing `Option` fields to `None` automatically):

```rust
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub format: String,
    pub quality: String,
    pub format_id: Option<String>,
    pub output_path: String,
    pub playlist_mode: Option<String>,
    pub audio_target: Option<String>,
    pub video_target: Option<String>,
    pub cookies_file: Option<String>,
    // New, all optional / backward-compatible:
    pub recode: Option<bool>,
    pub embed_thumbnail: Option<bool>,
    pub embed_metadata: Option<bool>,
    pub embed_chapters: Option<bool>,
    pub embed_subs: Option<bool>,
    pub sub_langs: Option<String>,
    pub sponsorblock: Option<String>, // "off" | "mark" | "remove"
    pub playlist_items: Option<String>,
    pub output_template: Option<String>,
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cd src-tauri && cargo test request_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ytdlp): add optional download-option fields to DownloadRequest"
```

## Task 3: `ytdlp/args.rs` — pure download-argument builder

**Files:**
- Modify: `src-tauri/src/ytdlp/args.rs`

- [ ] **Step 1: Write the failing tests**

Put this in `src-tauri/src/ytdlp/args.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test args::`
Expected: FAIL — `build_download_args` not found.

- [ ] **Step 3: Implement the builder**

Prepend to `src-tauri/src/ytdlp/args.rs` (above the test module):

```rust
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

/// Build the full yt-dlp argument vector for one download attempt.
/// `client` is the YouTube player_client to try (ignored when cookies are present).
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

    if let Some(tpl) = req.output_template.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
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
        if let Some(cookies) = req.cookies_file.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
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

    if let Some(items) = req.playlist_items.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--playlist-items".into());
        args.push(items.to_string());
    }

    let is_audio = is_audio_format(&req.format);
    let recode = req.recode.unwrap_or(false);

    if let Some(format_id) = req.format_id.as_deref().filter(|s| !s.is_empty()) {
        args.push("-f".into());
        args.push(format_id.to_string());
        if is_audio {
            push_audio_extract(&mut args, &req.format, &req.quality);
        } else if recode {
            args.push("--recode-video".into());
            args.push(req.format.clone());
        } else {
            args.push("--remux-video".into());
            args.push(req.format.clone());
        }
    } else if is_audio {
        push_audio_extract(&mut args, &req.format, &req.quality);
    } else {
        args.push("-f".into());
        if req.quality != "best" {
            args.push(format!("bv*[height<={h}]+ba/b[height<={h}]/b", h = req.quality));
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
        let langs = req.sub_langs.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("en.*");
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
```

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test args::`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ytdlp): pure build_download_args with remux-by-default and embed/sponsorblock"
```

## Task 4: `ytdlp/clients.rs` — fallback chain + retry predicate

**Files:**
- Modify: `src-tauri/src/ytdlp/clients.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_is_non_empty() {
        assert!(!YOUTUBE_CLIENT_CHAIN.is_empty());
    }

    #[test]
    fn retries_on_403_and_potoken() {
        assert!(should_try_next_client("ERROR: HTTP Error 403: Forbidden"));
        assert!(should_try_next_client("Some formats may be missing; po_token required"));
        assert!(should_try_next_client("Sign in to confirm you're not a bot"));
    }

    #[test]
    fn does_not_retry_on_disk_error() {
        assert!(!should_try_next_client("OSError: No space left on device"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test clients::`
Expected: FAIL — symbols undefined.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/ytdlp/clients.rs`:

```rust
/// Ordered YouTube player clients to try when no cookies are present.
/// Tunable; the durable fix for breakage is keeping yt-dlp updated.
pub const YOUTUBE_CLIENT_CHAIN: &[&str] = &["tv", "android_vr", "web_safari"];

/// Whether a failed attempt should be retried with the next client in the chain.
pub fn should_try_next_client(error: &str) -> bool {
    let l = error.to_lowercase();
    l.contains("http error 403")
        || l.contains("forbidden")
        || l.contains("po_token")
        || l.contains("po token")
        || l.contains("sign in to confirm")
        || l.contains("confirm you're not a bot")
        || l.contains("requested format is not available")
        || l.contains("unable to extract")
        || l.contains("nsig")
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test clients::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ytdlp): YouTube player-client fallback chain"
```

## Task 5: `ytdlp/errors.rs` — friendly error normalization

**Files:**
- Modify: `src-tauri/src/ytdlp/errors.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_check_suggests_cookies() {
        let msg = normalize_error("ERROR: Sign in to confirm you're not a bot", false);
        assert!(msg.to_lowercase().contains("cookies"));
    }

    #[test]
    fn unavailable_is_friendly() {
        let msg = normalize_error("ERROR: Video unavailable", false);
        assert!(msg.to_lowercase().contains("isn't available") || msg.to_lowercase().contains("not available"));
    }

    #[test]
    fn unknown_error_passes_through_trimmed() {
        let msg = normalize_error("  ERROR: weird thing happened  ", false);
        assert_eq!(msg, "ERROR: weird thing happened");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test errors::`
Expected: FAIL — `normalize_error` undefined.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/ytdlp/errors.rs`:

```rust
/// Map a raw yt-dlp stderr line to a friendly, actionable message.
/// The raw text is preserved when no known pattern matches.
pub fn normalize_error(raw: &str, had_cookies: bool) -> String {
    let l = raw.to_lowercase();

    if l.contains("confirm you're not a bot") || l.contains("sign in to confirm") {
        return "YouTube is asking to confirm you're not a bot. Import a fresh cookies.txt in Settings, then retry.".into();
    }
    if l.contains("private video") || l.contains("members-only") || l.contains("sign in") {
        return "This video requires sign-in. Import your cookies.txt in Settings and retry.".into();
    }
    if l.contains("video unavailable") || l.contains("not available") || l.contains("has been removed") {
        return "This video isn't available (it may be private, removed, or region-locked).".into();
    }
    if l.contains("po_token") || l.contains("po token") {
        return "YouTube needs a verification token for this format. Update yt-dlp (try the nightly channel in Settings) and retry.".into();
    }
    if l.contains("http error 403") || l.contains("forbidden") {
        return "Access was blocked (HTTP 403). Update yt-dlp in Settings and retry; if it persists, import cookies.".into();
    }
    if had_cookies && (l.contains("cookies") || l.contains("expired")) {
        return format!("{} Your saved cookies may be expired — export a fresh cookies.txt and re-import it.", raw.trim());
    }

    raw.trim().to_string()
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test errors::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ytdlp): friendly error normalization"
```

## Task 6: `ytdlp/progress.rs` — typed progress parsing

**Files:**
- Modify: `src-tauri/src/ytdlp/progress.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_progress() {
        let e = parse_progress_line("[download]  42.0% of ~10.00MiB at 1.20MiB/s ETA 00:05").unwrap();
        match e {
            ProgressEvent::Progress { percent, ref total, ref speed, ref eta } => {
                assert_eq!(percent, 42.0);
                assert_eq!(total, "10.00MiB");
                assert_eq!(speed, "1.20MiB/s");
                assert_eq!(eta, "00:05");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_destination() {
        let e = parse_progress_line("[download] Destination: /tmp/song.webm").unwrap();
        assert_eq!(e, ProgressEvent::Destination("/tmp/song.webm".into()));
    }

    #[test]
    fn parses_postprocess() {
        let e = parse_progress_line("[Merger] Merging formats into \"x.mp4\"").unwrap();
        assert!(matches!(e, ProgressEvent::PostProcess(_)));
    }

    #[test]
    fn ignores_unrelated() {
        assert_eq!(parse_progress_line("[youtube] abc: Downloading webpage"), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test progress::`
Expected: FAIL — symbols undefined.

- [ ] **Step 3: Implement**

Prepend to `src-tauri/src/ytdlp/progress.rs`:

```rust
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, PartialEq, Clone)]
pub enum ProgressEvent {
    Progress { percent: f64, speed: String, eta: String, total: String },
    Percent(f64),
    Destination(String),
    PostProcess(String),
}

fn progress_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%\s+of\s+~?([^ ]+)\s+at\s+([^ ]+)\s+ETA\s+([^ ]+)")
            .unwrap()
    })
}

fn percent_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\[download\]\s+(\d+(?:\.\d+)?)%").unwrap())
}

pub fn parse_progress_line(line: &str) -> Option<ProgressEvent> {
    if let Some(rest) = line.split("Destination:").nth(1) {
        let dest = rest.trim();
        if !dest.is_empty() {
            return Some(ProgressEvent::Destination(dest.to_string()));
        }
    }
    if line.starts_with("[ExtractAudio]")
        || line.starts_with("[Merger]")
        || line.starts_with("[VideoRemuxer]")
        || line.starts_with("[VideoConvertor]")
        || line.starts_with("[EmbedThumbnail]")
        || line.starts_with("[Metadata]")
        || line.starts_with("[SponsorBlock]")
    {
        return Some(ProgressEvent::PostProcess(line.to_string()));
    }
    if let Some(c) = progress_re().captures(line) {
        return Some(ProgressEvent::Progress {
            percent: c[1].parse().unwrap_or(0.0),
            total: c[2].to_string(),
            speed: c[3].to_string(),
            eta: c[4].to_string(),
        });
    }
    if let Some(c) = percent_re().captures(line) {
        return Some(ProgressEvent::Percent(c[1].parse().unwrap_or(0.0)));
    }
    None
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test progress::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(ytdlp): typed progress-line parser"
```

## Task 7: Wire `run_download_process` to the new modules + client fallback

**Files:**
- Modify: `src-tauri/src/ytdlp/mod.rs` (`run_download_process`, and the `start_download` retry block)

This is an **integration task** (no new unit test; verified by build + existing `start_download` retry logic + manual run).

- [ ] **Step 1: Import the new modules' items**

At the top of `src-tauri/src/ytdlp/mod.rs`, add:

```rust
use args::{build_download_args, is_youtube_url};
use clients::{should_try_next_client, YOUTUBE_CLIENT_CHAIN};
use errors::normalize_error;
use progress::{parse_progress_line, ProgressEvent};
```

Then delete the now-duplicated private helpers in `mod.rs` that moved into submodules: `audio_format_for_ytdlp`, `extract_destination`, `is_youtube_url`, `is_youtube_radio_mix_url`, and the old `re_progress`/`re_percent` regex usage inside the read loop. Keep `file_label`, `managed_cookies_file_path`, `normalize_download_error`→ replace with `normalize_error`, `looks_like_*` (still used by `list_formats`/`start_download`).

- [ ] **Step 2: Replace the argument-building block**

In `run_download_process`, replace everything from `let mut args = vec![...]` through the final `args.push(request.url.clone());` (the large block that built args inline) with a single call. Compute `has_cookies` first (as before), then:

```rust
let client = if is_youtube_url(&request.url) && !has_cookies {
    Some(YOUTUBE_CLIENT_CHAIN[attempt_index.min(YOUTUBE_CLIENT_CHAIN.len() - 1)])
} else {
    None
};
let args = build_download_args(&request, &ffmpeg_dir.to_string_lossy(), client, has_cookies);
```

Add an `attempt_index: usize` parameter to `run_download_process` (callers pass `0`). The client-fallback loop lives in `start_download` (Step 4).

- [ ] **Step 3: Replace the stdout read-loop body with `parse_progress_line`**

Inside the `while let Ok(Some(line)) = reader.next_line().await` loop, replace the `if line.starts_with("[download]") { ... }` cascade with:

```rust
if cancelled.load(Ordering::SeqCst) {
    return Err("Cancelled by user".into());
}
match parse_progress_line(&line) {
    Some(ProgressEvent::Progress { percent, speed, eta, total }) => {
        emit_job_progress(&app, JobProgressPayload {
            id: request.id.clone(),
            job_kind: "download".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "downloading".into(),
            percent, speed, eta, total_size: total,
            title: title.clone(),
            detail: output_path.clone().or_else(|| Some("Downloading media".into())),
            output_path: output_path.clone(),
            error: None,
        });
    }
    Some(ProgressEvent::Percent(percent)) => {
        emit_job_progress(&app, JobProgressPayload {
            id: request.id.clone(), job_kind: "download".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "downloading".into(), percent,
            speed: "-".into(), eta: "-".into(), total_size: "-".into(),
            title: title.clone(),
            detail: output_path.clone().or_else(|| Some("Downloading media".into())),
            output_path: output_path.clone(), error: None,
        });
    }
    Some(ProgressEvent::Destination(dest)) => {
        title = Some(file_label(&dest));
        output_path = Some(dest);
    }
    Some(ProgressEvent::PostProcess(detail)) => {
        emit_job_progress(&app, JobProgressPayload {
            id: request.id.clone(), job_kind: "download".into(),
            media_kind: media_kind_for_format(&request.format).into(),
            status: "converting".into(), percent: 100.0,
            speed: "-".into(), eta: "00:00".into(), total_size: "-".into(),
            title: title.clone(), detail: Some(detail),
            output_path: output_path.clone(), error: None,
        });
    }
    None => {
        let trimmed = line.trim();
        if !trimmed.starts_with('[') && Path::new(trimmed).exists() {
            output_path = Some(trimmed.to_string());
            title = Some(file_label(trimmed));
        }
    }
}
```

- [ ] **Step 4: Replace the cookie-only retry in `start_download` with a client + cookie fallback loop**

In `start_download`'s `tokio::spawn`, replace the single retry block with a loop that (a) tries each client index while `should_try_next_client(err)` and cookies are absent, then (b) falls back to dropping cookies (existing `looks_like_public_video_failure` behavior). Pass `attempt_index` to `run_download_process`. Keep all existing `emit_job_progress`/`emit_log` for cancelled/error/done. Skeleton:

```rust
let mut last_err: Option<String> = None;
let max_attempts = if is_youtube_url(&url) && current_request.cookies_file.is_none() {
    YOUTUBE_CLIENT_CHAIN.len()
} else { 1 };

for attempt in 0..max_attempts {
    match run_download_process(app_handle.clone(), current_request.clone(),
        Arc::clone(&pid), Arc::clone(&cancelled), attempt).await {
        Ok(()) => { last_err = None; break; }
        Err(e) => {
            if cancelled.load(Ordering::SeqCst) { last_err = Some(e); break; }
            if attempt + 1 < max_attempts && should_try_next_client(&e) {
                emit_log(&app_handle, format!("Retrying with next YouTube client ({})", e), "warn");
                last_err = Some(e);
                continue;
            }
            last_err = Some(e);
            break;
        }
    }
}

// Existing cookie-drop fallback: if last_err is Some, cookies present, and
// looks_like_public_video_failure(err), retry once with cookies_file = None.
```

Replace the existing `normalize_download_error(&request, stderr_excerpt)` call at the end of `run_download_process` with `normalize_error(&stderr_excerpt, has_cookies)`.

- [ ] **Step 5: Build and fix warnings**

Run: `cd src-tauri && cargo build`
Expected: builds. Remove any now-unused functions the compiler flags (`normalize_download_error` if fully replaced).

- [ ] **Step 6: Run the full test suite**

Run: `cd src-tauri && cargo test`
Expected: all green.

- [ ] **Step 7: Manual smoke test**

Run `pnpm tauri dev`, paste a public YouTube URL, download as MP4 (1080p) and as MP3. Confirm: progress updates, file appears, MP4 is not re-encoded (fast finish), Logs show client used.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "refactor(ytdlp): wire download to modular args/clients/errors/progress with client fallback"
```

## Task 8: Harden binary install — stream, validate status, atomic write, progress

**Files:**
- Modify: `src-tauri/src/binaries.rs`

- [ ] **Step 1: Write the failing test for the pure helper**

Add at the bottom of `src-tauri/src/binaries.rs`:

```rust
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
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib binaries`
Expected: FAIL — `part_path` / `ytdlp_download_url` undefined.

- [ ] **Step 3: Add the pure helpers + streamed `download_file` + a progress payload**

In `src-tauri/src/binaries.rs` add imports and helpers:

```rust
use futures_util::StreamExt;
use std::path::Path;
use tauri::Emitter;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BinaryProgress {
    pub binary: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: f64,
}

pub fn part_path(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    std::path::PathBuf::from(s)
}

pub fn ytdlp_download_url(os: &str, channel: &str) -> &'static str {
    match (os, channel) {
        ("windows", "nightly") => "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe",
        ("macos", "nightly")   => "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp_macos",
        (_, "nightly")         => "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp",
        ("windows", _)         => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("macos", _)           => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        (_, _)                 => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp",
    }
}

fn current_os() -> &'static str {
    if cfg!(target_os = "windows") { "windows" }
    else if cfg!(target_os = "macos") { "macos" }
    else { "linux" }
}
```

Replace the existing `download_file` with a streamed version that takes the app handle + a label:

```rust
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
    let resp = client
        .get(url)
        .header("User-Agent", "MediaToolsPro")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Download failed for {} (HTTP {})", label, resp.status().as_u16()));
    }
    let total = resp.content_length();
    let part = part_path(path);
    let mut file = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        let percent = total.map(|t| (downloaded as f64 / t as f64) * 100.0).unwrap_or(0.0);
        let _ = app.emit("binary-progress", BinaryProgress {
            binary: label.to_string(), downloaded, total, percent,
        });
    }
    drop(file);
    fs::rename(&part, path).map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **Step 4: Update callers in `install_ytdlp` / `install_ffmpeg`**

`install_ytdlp`: use `ytdlp_download_url(current_os(), channel)` (channel param added in Task 10; for now pass `"stable"`), and call `download_file(&app, url, &target, "yt-dlp").await?;`.
`install_ffmpeg`: the zip branch calls `download_file(&app, url, &tmp_zip, "ffmpeg").await?;`. Note `download_file` now writes `tmp_zip.part` then renames to `tmp_zip` — the rest of the zip extraction is unchanged.

- [ ] **Step 5: Run tests + build**

Run: `cd src-tauri && cargo test --lib binaries && cargo build`
Expected: PASS + builds.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(binaries): streamed install with progress, HTTP status check, atomic rename"
```

## Task 9: yt-dlp auto-update with throttle

**Files:**
- Modify: `src-tauri/src/binaries.rs`
- Modify: `src-tauri/src/lib.rs` (register command)

- [ ] **Step 1: Write the failing test for the throttle helper**

Add to `binaries.rs` tests module:

```rust
#[test]
fn should_check_respects_interval() {
    assert!(should_check_updates(None, 1000, 86_400));
    assert!(!should_check_updates(Some(1000), 1500, 86_400));
    assert!(should_check_updates(Some(0), 90_000, 86_400));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib binaries`
Expected: FAIL — `should_check_updates` undefined.

- [ ] **Step 3: Implement throttle + add `last_checked_unix` to the cache + the command**

Add the field to `VersionCache`:

```rust
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct VersionCache {
    ytdlp_version: Option<String>,
    ffmpeg_version: Option<String>,
    last_checked_unix: Option<u64>,
}
```

Add helpers + command:

```rust
pub fn should_check_updates(last_checked: Option<u64>, now: u64, interval_secs: u64) -> bool {
    match last_checked {
        None => true,
        Some(t) => now.saturating_sub(t) >= interval_secs,
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AutoUpdateResult {
    pub checked: bool,
    pub updated: bool,
    pub new_version: Option<String>,
}

/// Called on app launch. Throttled to once/day. Updates yt-dlp in place when a
/// newer stable release exists. Safe to call when offline (returns checked=false).
#[tauri::command]
pub async fn auto_update_ytdlp<R: Runtime>(app: AppHandle<R>, force: bool) -> Result<AutoUpdateResult, String> {
    let bin_dir = get_bin_dir(&app);
    let mut cache = read_cache(&bin_dir);
    let ytdlp_path = bin_dir.join(get_ytdlp_name());
    if !ytdlp_path.exists() {
        return Ok(AutoUpdateResult { checked: false, updated: false, new_version: None });
    }
    if !force && !should_check_updates(cache.last_checked_unix, now_unix(), 86_400) {
        return Ok(AutoUpdateResult { checked: false, updated: false, new_version: None });
    }
    let latest = match fetch_ytdlp_latest_version().await {
        Some(v) => v,
        None => return Ok(AutoUpdateResult { checked: false, updated: false, new_version: None }),
    };
    cache.last_checked_unix = Some(now_unix());
    write_cache(&bin_dir, &cache);

    let current = cache.ytdlp_version.clone();
    let is_newer = match (current.as_deref().and_then(parse_numeric_version), parse_numeric_version(&latest)) {
        (Some(c), Some(l)) => compare_versions(&l, &c) == std::cmp::Ordering::Greater,
        _ => current.as_deref() != Some(latest.as_str()),
    };
    if !is_newer {
        return Ok(AutoUpdateResult { checked: true, updated: false, new_version: None });
    }
    let url = ytdlp_download_url(current_os(), "stable");
    download_file(&app, url, &ytdlp_path, "yt-dlp").await?;
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&ytdlp_path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&ytdlp_path, perms).map_err(|e| e.to_string())?;
    }
    cache_single(&bin_dir, get_ytdlp_name());
    Ok(AutoUpdateResult { checked: true, updated: true, new_version: Some(latest) })
}
```

- [ ] **Step 4: Register the command** in `src-tauri/src/lib.rs` `generate_handler!` list, adding `binaries::auto_update_ytdlp,`.

- [ ] **Step 5: Run tests + build**

Run: `cd src-tauri && cargo test --lib binaries && cargo build`
Expected: PASS + builds.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(binaries): throttled yt-dlp auto-update command"
```

## Task 10: yt-dlp install channel (stable/nightly) + honest ffmpeg status

**Files:**
- Modify: `src-tauri/src/binaries.rs`

- [ ] **Step 1: Add `channel` param to `install_ytdlp`**

Change signature to `pub async fn install_ytdlp<R: Runtime>(app: AppHandle<R>, channel: Option<String>)` and compute `let channel = channel.as_deref().unwrap_or("stable");` then `let url = ytdlp_download_url(current_os(), channel);`. (Frontend passes `undefined` today → stable.)

- [ ] **Step 2: Make `check_binary_updates` honest about ffmpeg**

Leave ffmpeg fields but document that ffmpeg has no auto-update channel (static build). Keep `ffmpeg_update_available: false` but the Settings copy (Phase 3) will stop showing "Up to date" — for now add a doc comment. No behavior test needed.

- [ ] **Step 3: Build**

Run: `cd src-tauri && cargo build`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(binaries): yt-dlp stable/nightly channel selection"
```

## Task 11: Remove boilerplate + real README

**Files:**
- Modify: `src-tauri/src/lib.rs` (remove `greet`)
- Modify: `README.md`

- [ ] **Step 1: Delete the `greet` command** in `lib.rs` (the `#[tauri::command] fn greet` and its entry in `generate_handler!`).

- [ ] **Step 2: Build to confirm nothing referenced it**

Run: `cd src-tauri && cargo build`
Expected: builds.

- [ ] **Step 3: Replace README.md** with real content: what MediaToolsPro is, features (download + processing), how binaries are managed, dev setup (`pnpm install`, `pnpm tauri dev`), build, and platforms.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "chore: remove greet boilerplate, write real README"
```

## Phase 1 acceptance
- `cd src-tauri && cargo test` is green (args/clients/errors/progress/binaries/request + existing).
- `pnpm tauri dev`: public YouTube video downloads as MP4 (fast, remuxed) and MP3; a cookie-gated video gives a friendly error pointing to Settings; cancel works; Logs show the client used.
- yt-dlp auto-update command runs on demand without crashing offline.

---

# PHASE 2 — Frontend foundation (behavior-preserving refactor) — ROADMAP

> Expand to full bite-sized steps via `superpowers:writing-plans` before executing. Goal: no visible behavior change; same features, decomposed and testable.

- **Task A — Tooling:** add Vitest + `@vue/test-utils` as devDeps; `pnpm test` script; one trivial passing test to lock the harness.
- **Task B — Design tokens:** create `src/design/tokens.css` (color/space/radius/shadow/type CSS variables, light+dark); import in `main.ts`. No component changes yet.
- **Task C — `ui/` primitives:** extract `Button`, `Input`, `Select`, `Toggle`, `Card`, `ProgressBar`, `Modal`, `Field`, `SegmentedControl`, `Toast` from the inline markup currently repeated across `Downloader.vue`/`Settings.vue`. Snapshot/prop tests for each.
- **Task D — `useSettings` composable + tests:** move `collectSettings`/`applySettings`/`persistSettings`/legacy `audiotoolspro→mediatoolspro` migration out of `Downloader.vue`; unit-test persistence + migration.
- **Task E — `useQueue` composable + tests:** move queue state, concurrency limits, persistence, and progress-event handling; unit-test transitions (`waiting→downloading→done/error/cancelled`) and concurrency caps.
- **Task F — `useDownloads` / `useProcessing` / `useBinaries` / `useJobEvents`:** move the remaining domain logic; wire `binary-progress` + `job-progress` + `backend-log` listeners in `useJobEvents`.
- **Task G — Split views:** create `DownloadView.vue`, `StudioView.vue`, `QueueView.vue`, `SettingsView.vue` that consume the composables; `App.vue` renders them through the existing 3-tab UI *unchanged* for now. Delete `Downloader.vue`. Verify feature-parity manually against a checklist.

**Phase 2 acceptance:** `pnpm build` type-checks, `pnpm test` green, app behaves exactly as before, `Downloader.vue` gone, no file over ~400 lines.

---

# PHASE 3 — UX redesign + onboarding + new options — ROADMAP

> Expand before executing. Goal: professional, intuitive UI and the new download capabilities surfaced.

- **Task A — Sidebar shell:** replace the 3-tab `Header` with a left sidebar (Download / Studio / Queue / Settings) + content + footer status bar in `App.vue`; apply calm-pro tokens (drop full-screen radial gradients and stacked backdrop-blur).
- **Task B — Onboarding gate:** `OnboardingView.vue` shown on launch when yt-dlp/ffmpeg missing; one-click "Install both" with `binary-progress` bars; "Ready" state; downloads gated behind a clear CTA instead of raw errors.
- **Task C — Download view options:** surface toggles for recode vs remux, embed thumbnail/metadata/subs (+ langs), SponsorBlock (off/mark/remove), playlist range; persist via `useSettings`; pass through to `DownloadRequest`.
- **Task D — Settings redesign:** binaries cards with live install progress; **yt-dlp auto-update toggle (default ON)** + "Update now" + stable/nightly selector; honest ffmpeg status (installed/missing + reinstall, no fake "up to date"); advanced: player-client chain override.
- **Task E — Auto-update on launch:** call `auto_update_ytdlp(false)` on startup per the toggle; show a non-blocking toast/badge when updated or when an update is available.
- **Task F — Friendly errors in UI:** render normalized errors with the suggested action + an expandable raw detail; link to Settings where relevant.
- **Task G — Studio polish:** apply the design system to the waveform editor/processing panels; tighten layout and labels (no new DSP).

**Phase 3 acceptance:** first-run onboarding installs binaries; sidebar nav; all new download options work end-to-end; auto-update toggle honored; UI reads as professional in light + dark.

---

# PHASE 4 — Robustness & polish — ROADMAP

> Expand before executing.

- **Task A — Queue resume:** on launch, items left `waiting/downloading/processing` become `interrupted` with one-click **Retry**; terminal states stay as history. Tests in `useQueue`.
- **Task B — Accessibility pass:** focus states, keyboard nav for sidebar/queue, contrast audit of tokens (light+dark), `aria` on icon-only buttons, respects `prefers-reduced-motion`.
- **Task C — Empty/edge states:** empty queue, no-results format explorer, offline, disk-full; friendly copy everywhere.
- **Task D — Final verification:** run the manual checklist from the spec on macOS; `pnpm build` + `cargo test` green; update README screenshots/usage.

**Phase 4 acceptance:** interrupted downloads recover; a11y checks pass; the manual verification checklist passes on macOS.

---

## Cross-cutting conventions
- **Commits:** one per task step group; conventional-commit prefixes (`feat`/`fix`/`refactor`/`chore`/`test`).
- **TDD:** pure logic (Rust args/errors/progress/version; TS queue/settings/time) is test-first. Integration/UI verified by build + manual checklist.
- **DRY/YAGNI:** reuse `ui/` primitives and composables; do not add features outside the spec's goals.
- **Working tree:** every task ends with a green build/test and a commit.
