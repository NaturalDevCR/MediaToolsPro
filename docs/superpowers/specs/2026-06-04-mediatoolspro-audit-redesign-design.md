# MediaToolsPro — Audit, Reliability & Redesign Spec

**Date:** 2026-06-04
**Status:** Draft for review
**Owner:** Josue (NaturalDevCR)

---

## 1. Summary

MediaToolsPro is a Tauri 2 + Vue 3 + TypeScript desktop app that wraps `yt-dlp`
and `ffmpeg` to download media (YouTube and beyond) and process it (trim,
loudness-normalize, EQ, fades, split by silence/chapters/markers, with a waveform
editor). It is broader than MeTube, which is purely a download front-end.

This effort hardens the product across four fronts:

- **A — Download reliability** so YouTube downloads "just work" like MeTube, and
  keep working over time.
- **B — Robustness & correctness** (error handling, resume, tests, cleanup).
- **C — A professional, intuitive UI redesign.**
- **D — Decomposition of the 3,589-line god-component** that currently holds the
  entire frontend.

Scope decision (confirmed with user): **include all four fronts** in a single
phased delivery. No visual-companion mockups; design direction is specified in
text below.

## 2. Goals & non-goals

### Goals
1. YouTube (and generic yt-dlp) downloads succeed reliably for public, cookie-gated,
   and playlist content, and **self-heal over time** via automatic `yt-dlp` updates.
2. Fast downloads by default: **remux/merge** to the target container instead of
   always re-encoding.
3. MeTube-class convenience features: embed thumbnail/metadata/subtitles, optional
   SponsorBlock, playlist handling, friendly errors, first-run onboarding.
4. A clean, professional, intuitive UI with clear navigation and an onboarding flow.
5. A maintainable frontend: the god-component split into views + composables +
   reusable components, with a small design-token system.
6. Meaningful automated tests around the testable core (argument building, error
   normalization, settings/queue logic).

### Non-goals (YAGNI)
- No account system, cloud sync, or telemetry.
- No new media-processing capabilities beyond what exists (the processing suite is
  polished and bug-fixed, not expanded).
- No mobile target.
- No plugin marketplace / scripting.
- No bundled PO-token provider server (we improve client-fallback + cookies + keeping
  yt-dlp current; a full bgutil provider is explicitly out of scope for v1).

## 3. Key design decisions (and rationale)

| Decision | Choice | Why |
|---|---|---|
| Container handling | **Remux/merge by default**, re-encode only when user opts in | yt-dlp `--recode-video` always transcodes (slow, lossy). MeTube parity = fast, lossless. |
| YouTube extraction | **Ordered player-client fallback chain** + cookies + keep yt-dlp current | A single hardcoded `android_vr` is brittle; the durable fix is current yt-dlp + a fallback list. |
| Staying functional | **Auto-update yt-dlp** (on launch, throttled; default ON) + optional nightly channel | YouTube breaks yt-dlp regularly; this is the #1 cause of "stopped working". |
| Navigation | **Left sidebar** (Download / Studio / Queue / Settings) replacing 3 top tabs | Separates concerns; scales better; more professional. |
| Visual style | **Calm-pro**: solid surfaces, restrained accent, strong typography; drop heavy glassmorphism | Heavy blur + radial gradients read as "generic AI" and hurt legibility. |
| Frontend structure | **Full decomposition** into views/composables/components | 3,589-line single file blocks every other change; refactor first. |
| Errors | **Normalize to friendly, actionable messages**; keep raw in Logs | Raw last-line-of-stderr is cryptic for users. |
| Interrupted jobs | **Mark interrupted + one-click retry** on relaunch (not silent auto-resume) | Simple, safe, predictable. |
| Scope sequencing | Backend reliability first, then refactor, then redesign, then polish | Delivers the user's #1 goal early and de-risks the UI work. |

## 4. Architecture

### 4.1 Backend (Rust / `src-tauri/src/`)

Refactor `ytdlp.rs` (currently 823 lines mixing arg-building, process I/O,
progress parsing, and error logic) into focused, testable modules:

- `ytdlp/mod.rs` — Tauri commands (`list_formats`, `start_download`,
  `cancel_download`, cookie commands) and orchestration only.
- `ytdlp/args.rs` — **pure** functions: `build_download_args(&DownloadRequest, &Paths, &DownloadOptions) -> Vec<String>` and `build_list_formats_args(...)`. No I/O → unit-testable.
- `ytdlp/clients.rs` — ordered YouTube player-client fallback list and the
  retry policy (which errors trigger the next client).
- `ytdlp/progress.rs` — progress-line parsing (regex), returning a typed
  `ProgressUpdate`. Unit-testable against sample yt-dlp output.
- `ytdlp/errors.rs` — `normalize_error(raw: &str, ctx: &ErrorContext) -> FriendlyError`
  mapping known stderr signatures to actionable messages. Unit-testable.

`binaries.rs` changes:
- `download_file` → **streamed** download with progress events, **HTTP status
  validation** (error on non-2xx), write to `*.part` then atomic rename.
- Add an `update_ytdlp_if_needed` path used by auto-update; throttle GitHub API
  (cache `last_checked` in `.versions.json`).
- Replace the dead `ffmpeg_update_available: false` with honest behavior: report
  installed/missing + offer reinstall; do not claim "up to date" for a version we
  never compare.
- Add a `channel` (stable | nightly) option for yt-dlp downloads.

`DownloadRequest` gains optional fields (all default off / backward-compatible):
`embed_thumbnail`, `embed_metadata`, `embed_subs`, `sub_langs`, `sponsorblock`
(`off | mark | remove`), `embed_chapters`, `recode` (bool — force re-encode),
`playlist_items` (range string), `output_template`.

### 4.2 Frontend (`src/`)

Target structure:

```
src/
  views/
    DownloadView.vue       # URL → format explorer → confirm
    StudioView.vue         # processing (audio/video), waveform editor
    QueueView.vue          # unified queue + history
    SettingsView.vue       # binaries, defaults, advanced (clients, channel)
    OnboardingView.vue     # first-run binary setup
  composables/
    useBinaries.ts         # status, install (with progress), auto-update
    useDownloads.ts        # URL state, format explorer, build request
    useProcessing.ts       # probe, waveform, trim/split/eq state
    useQueue.ts            # queue state machine, concurrency, persistence, resume
    useSettings.ts         # persisted settings + legacy migration
    useJobEvents.ts        # listen() wiring for job-progress / backend-log
  components/
    ui/                    # Button, Input, Select, Toggle, Card, Modal,
                           # ProgressBar, Toast, Field, SegmentedControl
    download/  studio/  queue/   # feature components
  design/
    tokens.css             # CSS variables: color, spacing, radius, shadow, type
  stores/  types/  ...
```

`Downloader.vue` is deleted once its logic moves into the above. `App.vue` hosts
the sidebar layout, theme, global toasts, and the onboarding gate.

### 4.3 Design system

Defined in `src/design/tokens.css` as CSS variables, consumed via Tailwind:

- **Layout:** left sidebar (icon + label) + content + footer status bar.
- **Surfaces:** solid `--surface` / `--surface-2` with 1px `--border` and a single
  soft shadow tier. At most one subtle translucent layer (the sidebar) — no
  full-screen radial gradients or stacked backdrop-blur.
- **Color:** neutral base (zinc/slate), one tamed accent (cyan/sky), semantic
  status (emerald/amber/rose). Full light + dark parity.
- **Type:** Inter / system stack; explicit scale; **tabular numbers** for
  progress, sizes, ETAs.
- **Components:** the inline-everywhere buttons/inputs/cards become the `ui/`
  primitives so styling is consistent and changeable in one place.
- **Motion:** subtle and purposeful (progress shimmer stays); no gratuitous motion.

## 5. Feature specifications

### 5.1 Download engine (Front A)
- **Format/quality:** audio → `-x --audio-format <fmt> --audio-quality <n>`; video →
  `-f bv*[height<=Q]+ba/b[height<=Q]/b`. Exact `format_id` from the explorer still
  honored.
- **Container:** default `--remux-video <ext>` / `--merge-output-format <ext>`;
  `--recode-video` only when `recode=true`.
- **Client fallback:** try the configured client list in order (default chain, not a
  lone `android_vr`); advance to the next client on PO-token/403/availability
  errors; keep the existing cookie-present → default-extractor behavior.
- **Convenience flags (UI toggles, persisted):** embed thumbnail / metadata /
  chapters; subtitles (`--embed-subs` / `--write-subs` + `--sub-langs`);
  SponsorBlock (`off | mark | remove`).
- **Playlist:** explicit playlist/single toggle (existing) + optional
  `--playlist-items` range; keep radio/mix detection.
- **Output template:** configurable, default `%(title)s [%(id)s].%(ext)s`, optional
  per-playlist subfolder.
- **Progress:** typed parsing in `progress.rs`; emit `download | merging | recoding |
  embedding | done` phases to the queue.

### 5.2 Binary management & auto-update (Fronts A/B)
- Streamed install with live progress in onboarding/Settings; atomic write; HTTP
  status validation.
- yt-dlp **auto-update**: on launch (throttled to once/day), if enabled (default ON),
  fetch latest and replace; otherwise show a non-blocking "update available" badge.
  Manual "Update now" always available. Stable/nightly channel selector (advanced).
- Honest ffmpeg status (no fake "up to date").

### 5.3 Onboarding (Fronts A/C)
- On launch, if `yt-dlp` or `ffmpeg` is missing, route to `OnboardingView`: explain
  what they are, one-click "Install both" with progress, then a "Ready" state.
- Download actions are gated behind a clear CTA (not a raw error) when binaries are
  absent.

### 5.4 Errors (Front B)
- `normalize_error` maps known signatures → friendly message + suggested action,
  e.g.: bot-check/sign-in → "YouTube needs your cookies — import them in Settings";
  unavailable/geo → "This video isn't available (region or removed)"; outdated
  extractor → "Update yt-dlp (Settings) and retry". Raw error stays in Logs and an
  expandable detail.

### 5.5 Queue & persistence (Front B)
- Keep concurrency limits; unify into `useQueue` state machine.
- On relaunch, items left in active states become **`interrupted`** with a one-click
  **Retry**; completed/error/cancelled remain as history. Clear-history retained.

### 5.6 Processing studio (Front C polish)
- No new DSP features. Move logic into `useProcessing` + `StudioView`; fix any bugs
  surfaced during extraction; apply the new design system; keep waveform/trim/split
  UX, improved layout and clarity.

### 5.7 Cleanup (Front B)
- Remove boilerplate `greet` command. Replace the default-Tauri README with a real
  one (what it is, install, build, binaries, usage). Rename/retire the
  `Downloader.vue`→`ToolsView` confusion.

## 6. Testing strategy

- **Rust unit tests** (`cargo test`): `build_download_args` (audio/video/exact-id/
  recode/remux/playlist/flags matrices), `normalize_error`, `progress` parsing,
  `compare_versions`, playlist/radio detection.
- **TS unit tests** (Vitest, added as devDep): `useQueue` transitions & concurrency,
  `useSettings` persistence + legacy `audiotoolspro→mediatoolspro` migration, time
  parsing/trim math.
- **Manual verification checklist** (per release): mp3 audio, mp4 video (remux),
  1080p cap, playlist range, cookie-gated video, cancel mid-download, offline error,
  missing-binary onboarding, auto-update path.

## 7. Phased delivery plan (high level)

Detailed step-by-step lives in the implementation plan; phases & dependency order:

- **Phase 1 — Download engine + binaries (backend, highest priority, testable).**
  Refactor `ytdlp.rs` into modules; remux-by-default; client fallback chain;
  convenience flags; error normalization; streamed/validated binary install;
  yt-dlp auto-update; ffmpeg honesty. Ship with Rust unit tests. *Downloads work
  like MeTube even before the UI changes.*
- **Phase 2 — Frontend foundation (behavior-preserving refactor).** Design tokens +
  `ui/` primitives; split `Downloader.vue` into views/composables/components; add
  Vitest + first store tests. No feature change yet.
- **Phase 3 — UX redesign + onboarding + new download options.** Sidebar nav;
  Download/Studio/Queue/Settings views; first-run onboarding; surface embed/
  SponsorBlock/playlist/auto-update controls; friendly error display; calm-pro
  styling.
- **Phase 4 — Robustness & polish.** Queue interrupt+retry on relaunch; remove
  `greet`; real README; accessibility & dark-mode pass; final manual-verification
  checklist.

Each phase is independently shippable and leaves the app working.

## 8. Risks & mitigations

- **YouTube changes break extraction** → auto-update yt-dlp + client fallback +
  nightly option; surface clear "update & retry" errors.
- **Refactor regressions** in the huge component → Phase 2 is explicitly
  behavior-preserving; move logic in small, verifiable slices; lean on the new
  store tests.
- **Streamed-install edge cases** (partial files) → `.part` + atomic rename + status
  validation.
- **Scope size** → strict phase boundaries; each phase ships a working app; non-goals
  enforced.

## 9. Open questions (defaults chosen; flag if you disagree)

1. Auto-update yt-dlp **ON by default** — acceptable? (Alternative: prompt-only.)
2. Accent color stays **cyan/sky** (tamed) — keep brand, or pick another?
3. SponsorBlock included as **opt-in off-by-default** — fine, or omit for v1?
