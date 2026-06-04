# MediaToolsPro

MediaToolsPro is a Tauri 2, Vue 3, TypeScript, and Rust desktop app for downloading and preparing media. It wraps `yt-dlp` for downloads and `ffmpeg` for audio/video processing, with queue management, logs, format inspection, and a processing studio in one local app.

## Features

- Download public and cookie-gated media through `yt-dlp`
- Audio extraction to MP3, WAV, FLAC, M4A, AAC, OGG, and Opus
- Video downloads with remux/merge by default and optional re-encode support
- Playlist and single-video modes, exact format selection, and quality caps
- Optional metadata, thumbnail, chapters, subtitles, and SponsorBlock flags
- Processing tools for trim, loudness normalization, EQ, fades, splitting, and waveform-assisted workflows
- Local queue, progress updates, cancellation, history, and backend logs
- Managed `yt-dlp` and `ffmpeg` binaries with install, delete, update checks, and streamed install progress

## Binary Management

The app stores managed binaries in its Tauri app data directory under `bin/`.

- `yt-dlp` can be installed from the stable or nightly release channel.
- `yt-dlp` update checks compare the installed cached version with the latest upstream release.
- `ffmpeg` is installed from platform-specific static builds where supported. The app reports installed/missing status and cached version text, but does not claim an app-managed ffmpeg update channel.
- Downloads are written to `*.part` files first, validate HTTP success status, emit progress events, and rename into place when complete.

## Development

Install dependencies:

```bash
pnpm install
```

Run the desktop app in development:

```bash
pnpm tauri dev
```

Run the frontend type-check and Vite build:

```bash
pnpm build
```

Run Rust tests:

```bash
cd src-tauri
cargo test
```

Build the desktop app:

```bash
pnpm tauri build
```

## Project Layout

- `src/` contains the Vue frontend.
- `src-tauri/src/` contains Rust commands, binary management, download orchestration, queue state, and media processing.
- `src-tauri/src/ytdlp/` contains the modular yt-dlp download code: argument building, client fallback policy, progress parsing, and friendly error normalization.

## Platforms

MediaToolsPro targets the desktop platforms supported by Tauri 2. Current development is focused on macOS, with Windows and Linux paths present for managed binaries where the upstream release assets are available.
