# OrangeLoaf — Implementation Plan

> **Naming note (2026-07-29)**: this project was originally named "Voltaic" throughout M0–M15. It was renamed to **OrangeLoaf** — matching this repository's own name (`orange-loaf`) — after learning "Voltaic" collides with an existing, established GitHub organization's name. Every reference throughout this document, the codebase, config, and env-var names (`ORANGELOAF_CONFIG`/`_QUEUE`/`_HISTORY`/`_PROFILES`, previously `VOLTAIC_*`) was swept and updated; nothing about the actual design, architecture, or milestone history changed, only the name.

> **Clean-room notice**: [`cdgriffith/FastFlix`](https://github.com/cdgriffith/FastFlix) (MIT) was studied read-only, cloned locally to `temp/FastFlix` (gitignored, not part of this repo, kept only as a local research reference) purely to understand *behavior* — how a mature 30-encoder FFmpeg GUI has to work. **No FastFlix source, assets, strings, or branding are copied into this codebase.** OrangeLoaf is an original implementation from the ground up: a Tauri 2 (Rust) + Svelte 5 + DaisyUI 5 desktop video encoder, with its own name, its own design system, and its own code. Section 1 below is research notes on the prior art's behavior (kept for engineering reference — exact flags, gotchas, edge cases worth knowing about); it is not a spec for what to name or brand anything in OrangeLoaf itself.

## 0. TL;DR

The studied prior art is a PySide6 desktop app that wraps FFmpeg and ~30 encoder backends. It is architecturally sound (clean plugin-per-encoder pattern, a real settings/queue/profile/history system, 30 working encoder integrations) but its **UI is a dense, flat, tab-soup Qt Fusion interface with no visual hierarchy** — that's the entire complaint, and it's a legitimate one (see `docs/gui_preview.png` in the local reference clone). OrangeLoaf's job:

1. **Re-implement, clean-room, essentially all of the underlying behavior** (command construction, HDR pipeline, queue semantics, settings schema) — this is 6+ years of encoder-specific knowledge encoded in ~30 command builders and is the actual hard part of this app. "Re-implement" means: build our own code from an understanding of *what it needs to do and why*, verified against real ffmpeg/encoder documentation — not transcription.
2. **Fix acknowledged gaps** while porting: no diagnostic info on encode failure, no aggregate queue progress, profiles are create-only, theme/language changes require restart, no cross-process liveness check, several dead/unreachable features (Large Preview, HDR10+ Inject window, the `AudioSelect` stub, VAAPI two-pass).
3. **Replace** the entire GUI layer with a Svelte 5 + DaisyUI 5 app on a calm, premium, information-dense-but-organized dark design system (already built and screenshotted — see §2).
4. **Modernize** the runtime: single Tauri process instead of Python's 2–3 process/multiprocessing-queue dance, real `ffmpeg -progress` parsing instead of stderr string-splitting, tagged enums instead of Pydantic Union-coercion workarounds.

This plan is long because the source app is genuinely feature-rich (30 encoders × HDR10/HDR10+/DV × audio/subtitle/queue/profile/history/i18n systems). Sections 1–2 summarize what exists and why the redesign looks the way it does; Section 3–5 define the new architecture; Section 6 is the actual step-by-step build order; Sections 7–9 are testing, open decisions, and reference appendices (encoder parameter tables, full control-parity checklist).

---

## 1. What FastFlix Actually Is (current-state analysis)

### 1.1 Process architecture

Contrary to its own `CLAUDE.md`, FastFlix is **two OS processes, not three**:

- **Root process** (`entry.py`) — runs `conversion_worker.py`'s queue loop *in itself* (not a spawned `Process`). This is the process that actually launches ffmpeg/x265/NVEncC as subprocesses via `psutil.Popen` (`command_runner.py:BackgroundRunner`).
- **GUI child process** (`application.py`) — the PySide6/Qt event loop, spawned via `multiprocessing.Process`. Never touches ffmpeg directly.
- Three unbounded `multiprocessing.Queue()`s (`worker_queue`, `status_queue`, `log_queue`) are the *entire* IPC surface — plain tuples/namedtuples, no shared business-logic classes.

Multi-step jobs (2-pass encodes, palette-then-GIF) are **not** a loop inside the worker — the worker runs exactly one command and reports `"complete"`; the **GUI process** decides whether more commands remain (`video.status.current_command`) and fires the next one as a brand-new `"execute"` request. `BackgroundRunner` never knows about a list.

Known gaps we will *not* reproduce: no heartbeat in either direction (if the root process dies, the GUI hangs silently with no notification); cancellation is instant SIGTERM+SIGKILL with no grace period; a `Request` namedtuple defined in a PySide6-importing module gets unpickled in the "keep Qt out of this process" root process, silently importing PySide6 there anyway.

**Rust target:** collapse to **one process**. `tokio::task`s replace both the worker loop and the Qt marshalling threads; `window.emit()` replaces all three queues. See §4.1–4.2.

### 1.2 Media probing & HDR detection (`flix.py`)

- `ffprobe -v quiet -loglevel panic -print_format json -show_format -show_streams <file>` → parsed into an untyped `python-box` dict (FastFlix never defines a schema for raw ffprobe JSON, only for its own derived track models).
- Cover art detection: a `codec_type == "video"` stream is reclassified as an attachment if `disposition.attached_pic` or `tags.MIMETYPE` starts with `image`.
- **HDR10 static metadata**: mastering-display + CLL come from ffprobe side-data (`-show_frames -read_intervals %+#1`, first frame only). The chromaticity/luminance rescale formula (`convert_mastering_display`) divides ffprobe's `upper/lower` fraction to a base of **50,000** (chromaticity) or **10,000** (luminance) — this exact formula is x265's own string convention and must be ported verbatim; SVT-AV1 needs the *inverse* rescale (divide back down to a `0.xxxx` float) — a likely bug spot if copied carelessly.
- **HDR10+ detection**: ffprobe side-data scan (any codec) as the universal fallback; for HEVC specifically, if `hdr10plus_parser`/`hdr10plus_tool` is configured, a two-process pipe (`ffmpeg ... -f hevc - | hdr10plus_tool --verify -`) is used instead. Note: the *detection* invocation and the *extraction* invocation (§1.3) use two different, inconsistent argv orderings for the `extract` subcommand in the Python source — **verify the real CLI grammar once and use one consistent order** in the port, don't copy both.
- **Dolby Vision is never detected or parsed by FastFlix at all.** It's delegated 100% to rigaya binaries' own `--dolby-vision-rpu copy`. Don't build DV parsing into the Rust core either.
- Bit-depth guessing, rotation (tag + side-data, swaps width/height for 90/270°), cropdetect (`-vf cropdetect=round=2`, parsed by splitting on the last `=`), interlace detection (`-vf idet`, regexes `TFF:\s+(\d+)` / `BFF:\s+(\d+)` / `Progressive:\s+(\d+)`) are all separate ffprobe/ffmpeg shell-outs in this same file.

### 1.3 The `Command` abstraction & 30-encoder command construction

Full detail lives in the research below; the essential shape:

```python
class Command(BaseModel):
    command: Union[List[str], str]   # argv, or (only for gifski) a shell-pipe string
    name: str = ""                   # display label only
    exe: str = None                  # display only — never read for dispatch
    shell: bool = False              # the only field besides `command` actually used by the runner
    uuid: str
```

`build(fastflix) -> List[Command]` per encoder. Sequencing across a multi-command list is **not** a loop anywhere — the GUI dispatches index 0, waits for `"complete"`, dispatches index 1, etc. Every command in a job's list shares one `work_dir`, which is how relative pass-log filenames and GIF's temp palette PNG survive across command boundaries.

**Shared foundation** (`encoders/common/helpers.py`) — used by ~20 of 30 encoders:
- `generate_ffmpeg_start()` / `generate_filters()` / `generate_ending()` / `generate_all()` assemble ordered argv fragments. Order matters and is enumerated in full in Appendix A.
- **Filter chain, fixed order**: deinterlace → crop → scale+setsar → rotate → vflip → hflip → speed → deblock → denoise → eq → [VAAPI hwdownload/hwupload wrap] → HDR tonemap → custom filters (this is the "scale before palettegen" rule CLAUDE.md alludes to) → subtitle burn-in wrap. **Picture-based subtitle burn-in (`overlay`) must happen BEFORE scaling; text-based (`subtitles=`) filter must happen AFTER** — verbatim source comment: *"You have to overlay first for it to work when scaled."* This ordering rule is the single most important piece of business logic in the filter system and needs a dedicated test matrix in the port.
- **Two-pass**: two `Command`s, differing only in the codec-specific "middle" fragment. Pass 1 discards audio/subs (`-an -sn -dn`), writes to the platform null device inside a per-codec-specific *discardable* container (mp4/matroska/webm — picking the wrong one makes ffmpeg's pass-1 muxer error even though the output is thrown away). x265/SVT-AV1 use a **relative, cwd-dependent** pass-log filename (relies on `cwd=work_dir` matching); x264/others use an **absolute** path. Rigaya encoders don't do ffmpeg-style two-pass at all — `--multipass` is one native flag, internal to the exe.

**Three structurally distinct encoder families** (don't force one abstraction over all three):
1. **ffmpeg-native, filter-graph-based** (x265, x264, av1_aom, svt_av1(+avif), rav1e, vp9, vvc, VAAPI ×4, VideoToolbox ×2, ffmpeg-nvenc ×2, webp) — ffmpeg does everything.
2. **External-exe** (nvencc/qsvencc/vceencc × hevc/avc/av1 = 9 encoders) — ffmpeg plays **no role at all**; the rigaya binary opens the source directly and writes the muxed output itself, using its own native VPP flags for resolution/crop/rotate/flip (note: **`--crop` order is L,T,R,B — different from ffmpeg's crop filter's W:H:L:T**) and its own `--avhw`/`--avsw` decode path.
3. **Hybrid pipe** (gifski only) — ffmpeg produces a raw `yuv4mpegpipe` stream piped into the `gifski` binary. This is the only place `shell=True` + a string command exists in the whole codebase.

**HDR10/HDR10+/DV re-injection is per-family, not unified** — four genuinely different string syntaxes:

| Family | HDR10 static | HDR10+ dynamic | Dolby Vision |
|---|---|---|---|
| x265 | `-x265-params master-display=G(x,y)B(x,y)R(x,y)WP(x,y)L(max,min):max-cll=N,M:hdr10=1` | `-x265-params dhdr10-info='<json>'` | not supported |
| SVT-AV1 | `-svtav1-params mastering-display=...:content-light=...:enable-hdr=1` (values **rescaled** ÷50000/÷10000) | not supported | not supported |
| rav1e | `-rav1e-params mastering_display=...:content_light=...` | not supported | not supported |
| VVC | `-vvenc-params MasteringDisplayColourVolume=Gx,Gy,Bx,By,Rx,Ry,WPx,WPy,Lmax,Lmin:MaxContentLightLevel=N` | not supported | not supported |
| AV1 (AOM), VP9, x264, VAAPI, VideoToolbox, ffmpeg-NVENC | none emitted | none | none |
| rigaya ×9 | `--master-display ... --max-cll N` | `--dhdr10-info copy` (source passthrough, no JSON needed) | `--dolby-vision-rpu copy --dolby-vision-profile copy [--dolby-vision-rpu-prm crop=true]` |

`av1_aom`'s HDR handling is a **log line only** ("passthrough will be handled automatically by FFmpeg 8.0+") — no explicit metadata injection at all, unlike its two AV1 siblings. Flag for empirical verification before assuming parity in the port.

**Audio/subtitle rendering** is per-family too: ffmpeg native uses `-map`/`-c:N`/`-metadata:s:N`/`-disposition:N`; rigaya uses `--audio-copy`/`--audio-stream id?layout`/`--sub-copy`/`--vpp-subburn`. **Critical gotcha**: rigaya track IDs are **1-based, per-media-type ordinals** (`get_stream_pos()` remaps), not ffmpeg's global `0:N` stream index — naively reusing ffmpeg's addressing convention will silently pick the wrong track once more than one stream type is mixed. MP4 containers need `mov_text` for subtitles, not `copy`.

**Naming** (`naming.py`) is a two-phase template system: pre-encode variables (`{source}`, `{encoder}`, `{crf}`, `{hdr}`, `{resolution}`, …) resolve at queue time; post-encode variables (file size, achieved bitrate, elapsed time) are baked into the **on-disk filename** as placeholder tokens (`FFFSIZE`, `FFVIDBIT`, …) and literally string-replaced after encoding finishes, because the file must exist with *some* name before ffmpeg runs but stats are only known afterward. This placeholder-then-rename trick is worth keeping as-is.

**External-binary discovery** (`gpu_detect.py`, `program_downloads.py`, `rigaya_helpers.py`): Windows-only GPU vendor detection via `wmi`; rigaya exe/hdr10plus_tool/ffmpeg auto-download only on Windows, via GitHub Releases API + 7-Zip/zip extraction + a size sanity check. **No Linux/macOS auto-download or GPU detection exists in the source app at all** — don't imply parity there without a deliberate decision.

Full 30-encoder comparison table, parameter mappings, and the shared-helper call order are in **Appendix A**.

### 1.4 GUI inventory (what must reach parity)

The legacy main window: menu bar → top bar (Source / Encoder dropdown / Profile dropdown / Add-to-Queue / Convert) → a left file-info column + center preview (static-frame thumbnail + scrub slider, regenerated via a backgrounded ffmpeg call on slider-*release*, not live) → a **first tab strip** (Size / Start-End Time / Crop / Options) → a **second tab strip** of up to 11 tabs (**Quality, Audio, Subtitles, Data, Cover, Advanced, Source Details, Raw Commands, Encoding Status, Encoding Queue**, +Debug in dev mode) → a single global status bar.

The densest screen in the whole app is the per-encoder **Quality** tab: x265's alone has **~28 interactive controls + 6 read-only labels** (preset/tune/profile/bit-depth, CRF-vs-bitrate mode box with a passes dropdown, adaptive quantization, frame threads, muxing queue size, 6 HDR10/HDR10+ checkboxes+fields, B-frames/B-adapt, intra-refresh/smoothing, lossless, free-form params field, custom ffmpeg options, 2 guide hyperlinks) — this single screen is the entire reason the legacy UI "sucks": it's a flat grid with no grouping, and every one of the 30 encoder panels is built from the same shared widget factory (`SettingPanel._add_combo_box/_add_text_box/_add_check_box/_add_modes`), just with different field lists.

**Live command preview has no debouncing** — every keystroke/toggle synchronously rebuilds the full command list (it's just string formatting, cheap). Only the *thumbnail* regeneration is hash-gated/backgrounded, because that's the one genuinely expensive operation.

**Real gaps confirmed in the shipped app** (fix these, don't port them):
- **No aggregate queue progress** anywhere — no "N of M done," no overall bar. Only per-item status text and one global bar for whatever's currently running.
- **Profiles are create-only** — no rename/duplicate/edit; to "edit" a profile you delete and recreate it under the same name.
- **Theme and language changes require an app restart** — QSS is applied once at startup; `language.py`'s `lru_cache`d translator never re-reads config after import.
- **Dead/unreachable features**: `LargePreview` (no menu item or call site anywhere), `HDR10PlusInjectWindow` (Tools-menu entry commented out), the `AudioSelect` stub in `windows/audio_select.py` (empty — don't confuse with the *real*, used `AudioSelect` inside `profile_window.py`).
- **VAAPI two-pass code exists but is fully commented out** in all 4 VAAPI encoders — single-pass only today regardless of settings.
- A `priority` config field is persisted but never actually read by the worker — silently a no-op.

Full literal screen/control inventory (every dropdown, checkbox, and button, with what it does) is **Appendix B** — treat it as the feature-parity checklist; nothing in the rewrite ships as "done" until every row has a home in the new IA.

### 1.5 Packaging reality check

**FFmpeg/FFprobe are never bundled** — confirmed three ways (WINDOWS_BUILD.md says so outright, the app ships a Settings→Application Locations tab with path fields, and the Help menu has "Download Stable/Nightly FFmpeg" actions that fetch a build into `%APPDATA%` *at runtime*). This is almost certainly because **x264/x265 are GPL** and FastFlix's own license is MIT — statically bundling GPL-linked binaries into an MIT-licensed installer is a real licensing question, not an oversight. Same story for the 9 rigaya exes and gifski: always optional, user-supplied or runtime-downloaded, never packaged. The NSIS installer does Start-Menu shortcuts + registry uninstall entries only — no file-type associations, no desktop shortcut. **No codesigning or notarization happens anywhere in CI** (the GitHub Actions steps for it exist but are commented out) — an unsigned/non-notarized app is the actual current shipping state, not a rewrite regression to avoid.

---

## 2. Design direction

### 2.1 Style anchor

Style system: SuperDesign library prompt **"Dashboard UI: Dark Linear-Style Developer Issue Tracker"** — chosen over a "technical/cybernetic console" alternative (acid-green, 0px-radius brutalism) because it more directly attacks FastFlix's actual failure mode: *zero visual hierarchy*. The anchor's whole design language is "hierarchy from size/weight, never color; a single rationed accent; hairlines instead of boxes-in-boxes" — which is a precise, structural fix for a UI whose entire problem is 30+ same-weight checkboxes competing for attention on one screen.

Full token spec: **`design/design-system.md`**. Summary:

| Token | Value |
|---|---|
| App / sidebar / main canvas | `#08090a` / `#0c0d0f` / `#0b0c0e` |
| Raised surface | `#111318` |
| Hairline border | `rgba(255,255,255,0.07)` — no drop shadows anywhere in the base UI |
| Primary text / secondary / muted | `#f4f5f8` / `#b9bdc6` / `#7d828c` |
| UI typeface | Inter (only) |
| Technical/mono typeface | JetBrains Mono — commands, codec ids, timecodes, logs |
| Accent (single, rationed) | `#5e6ad2` — the one primary button, active nav tint, focus rings, progress fill. Never decorative. |
| Status semantics | queued `#7d828c` (dashed) · encoding `#f2c94c` (amber, pulsing) · done `#4ade80` · failed `#eb5757` · paused `#5e6ad2` outline |
| Radius / density | 6–8px controls, 4px chips; rows ~36–40px; 16–20px section padding |
| Motion | 120–180ms ease-out, no bounce |

### 2.2 Verified mockups

Built with SuperDesign against the token file above, rendered with real DaisyUI/Tailwind (via CDN) and screenshotted through headless Chrome for visual verification — not hand-waved.

**Convert screen** (`design/mockups/convert-screen.html` / `.png`):

![Convert screen mockup](../design/mockups/convert-screen.png)

**Queue screen** (`design/mockups/queue-screen.html` / `.png`) — note the segmented aggregate-progress bar and per-group status counts (a direct fix for the "no aggregate progress" gap in §1.4), and every row's disclosure chevron: the actively-encoding row is shown expanded, which is where the legacy "Encoding Status" tab's live log now lives — see §2.4:

![Queue screen mockup](../design/mockups/queue-screen.png)

Both are throwaway static references (Tailwind CDN + `iconify-icon`), not production code — their job is to lock the visual language before Svelte work starts. Open them directly in a browser to inspect spacing/interaction details beyond what the screenshots show.

### 2.3 Information-architecture changes vs. the legacy app

| Legacy | New | Why |
|---|---|---|
| 2 stacked horizontal tab strips (4 + 11 tabs) | Left icon-rail (Convert/Queue/History/Profiles/Settings) + a **vertical accordion** of sections inside Convert (Quality/Audio/Subtitles/Filters/HDR·Advanced/Command Preview), one open at a time | The legacy layout makes 15 destinations equally salient at once. An accordion + rail makes exactly one thing salient, which is the actual fix for "no hierarchy." |
| Data tab separate from Cover tab | Folded into a "Filters/Attachments" grouping (still functionally distinct, see Appendix B — **Data ≠ Cover, both must be preserved**) | Fewer top-level slots without dropping capability |
| Global single-line status bar for "whatever's running now" | Per-row status in Queue + a **new** aggregate segmented progress bar | Net-new capability, addresses a real, confirmed gap |
| Profiles: create-only | Add rename/duplicate/save-changes | Net-new capability, addresses a real, confirmed gap (§1.4) — **flag as a product decision** if strict parity-only is preferred instead, see §8 |
| Restart-required theme/language | DaisyUI `data-theme` hot-swap; i18n store re-renders live | Free upgrade from moving off Qt QSS / `lru_cache`d translator |
| x265 Quality tab: ~28 controls in one flat grid | Same ~28 controls, grouped into "Core" (always visible) + "Advanced" (collapsed) + a "Custom params" escape hatch, using one declarative field-schema renderer (see §5.4) instead of one bespoke layout per encoder | Same information, organized; also makes adding encoder #31 a data change, not a new screen |
| "Encoding Status" tab (global, one at a time: live log + time-left/elapsed/size-estimate) | Per-row disclosure on every Queue item — expanding a row reveals the same stats header + a live JetBrains-Mono log block, scoped to that job | The legacy tab only ever shows *whichever job happens to be running*; a per-row home scales to N concurrent queue items and survives you moving on to configure the next source in Convert (see §2.4) |

### 2.4 Where the live encoder log lives

The legacy "Encoding Status" tab is global and singular — it shows whatever is currently running, in a tab that has nothing to do with the job it's describing once you've moved on to configuring the next video. That mapping breaks down the moment there's a real queue with an aggregate progress bar (§2.3): the log is a property of *a job*, not of *the Convert screen*, so it moves to where jobs live.

**Decision: the Queue view owns it.** Every row gets a disclosure chevron (see the updated `design/mockups/queue-screen.png` — the actively-encoding row is shown expanded as the example). Expanding a row reveals, in place, on the same raised `#111318` surface:
- A slim stats header — `Time Left` / `Elapsed` / `Size Estimate`, the same three values the legacy tab showed, in muted mono numerals — plus the legacy's `Hide NAL unit messages` toggle (kept verbatim; it's specific, meaningful domain terminology to anyone who's hit x264/x265's noisy per-frame NAL logging) and a `Copy Log` action.
- A dark `#08090a` scrollable terminal block (JetBrains Mono, per the design system's existing "log/debug output" token — no new tokens needed) rendering real ffmpeg stderr lines, with the live `frame=…fps=…time=…bitrate=…speed=…` progress line always pinned as the most recent. Warning-level lines get a single-line amber tint (`#f2c94c`) inline — not a full-block treatment — matching how the design system already rations color to small, specific signals rather than whole-surface fills.
- Queued rows expand to an empty/"nothing to show yet" state; completed/failed rows expand to their **final** captured log (failed rows' error reason, already shown collapsed per §1.4, is the first line of that log, not a separate field) — so history-diving into *why* something failed doesn't require a different UI than watching something live.

**Convert screen's "Command Preview" section stays pre-flight-only** — it shows the command that *will* run, which remains useful for sanity-checking/copying before you commit, and doesn't try to duplicate a live log for a job that may no longer be the one on screen. If the video currently loaded in Convert is the one actively encoding, that section shows a small "Encoding in progress — view live log in Queue" link rather than a second copy of the log.

---

## 3. Target stack

Confirmed current as of 2026-07 (all bleeding-edge, all GA — not previews):

| Layer | Choice | Notes |
|---|---|---|
| Desktop shell | **Tauri 2.10** | Stable; ~600KB minimal app, 20–100MB idle RAM, native WebView (no bundled Chromium) |
| Backend language | **Rust**, edition 2024 | async via `tokio` |
| Frontend framework | **Svelte 5** (runes: `$state`/`$derived`/`$effect`/`$props`) | no SvelteKit — this is a single-window desktop app; SvelteKit's routing/SSR is cruft we don't need. Plain Svelte + Vite. |
| CSS / components | **Tailwind CSS 4** + **DaisyUI 5** | DaisyUI 5 is a from-scratch rewrite: zero-dependency, per-component CSS, a new theming engine (CSS custom properties, matches our token table in §2.1 directly) |
| Language | **TypeScript 7.0** (native Go compiler, GA July 8 2026, ~10x faster typecheck) | ships as the standard `tsc`/LSP now, not a separate preview package |
| Package manager / JS runtime | **Bun 1.3.x** | `bun install`, `bun run dev`, `bunx` in place of npm/npx; Bun's Vite integration is stable |
| Bundler | **Vite** (via `@tauri-apps/cli` + `@sveltejs/vite-plugin-svelte`) | |
| Icons | `@iconify/svelte` or `lucide-svelte` (mockups use Iconify `lucide:*` — keep the same icon set for continuity with the approved design) | |

### 3.1 Key Tauri plugins

| Plugin | Use |
|---|---|
| `tauri-plugin-shell` | Spawn ffmpeg/ffprobe/rigaya binaries; `sidecar()` API if we bundle (see §3.2) |
| `tauri-plugin-dialog` | Native open/save file & folder pickers (replaces every `QFileDialog` call site) |
| `tauri-plugin-fs` | Scoped filesystem access for thumbnails, queue/history/profile files |
| `tauri-plugin-store` | Simple KV persistence, OR hand-roll with `serde_json`/`serde_yaml` directly (config schema is complex enough — see §4.10 — that a typed Rust struct + manual save/load is likely cleaner than the generic store plugin) |
| `tauri-plugin-window-state` | Remember window size/position (parity with the legacy "restore last window state" behavior implicit in Qt) |
| `tauri-plugin-updater` | Replaces the "Check for Newer Version" GitHub-releases check |
| `tauri-plugin-single-instance` | Decide explicitly whether multiple app instances may run concurrently (legacy Python allows it and defends the queue file with an advisory lock — see §8) |
| `tauri-plugin-os` | GPU/platform detection groundwork |

### 3.2 FFmpeg distribution — explicit decision needed (see §8)

Two real options, not a default:
- **A. BYO ffmpeg** (parity with legacy): detect on PATH/common install dirs, offer a guided download (mirrors `program_downloads.py`'s GitHub-release-fetch pattern), surface status in Settings exactly like the legacy "Detected External Programs" list.
- **B. Bundle ffmpeg as a Tauri sidecar** (`tauri.conf.json` → `bundle.externalBin`, per-target-triple binaries, spawned via `app.shell().sidecar("ffmpeg")`): a real UX improvement (zero-setup), but reopens the GPL-bundling question the legacy app deliberately avoided (§1.5). Needs a licensing decision before being the default, not a purely technical one.

Recommendation: ship **A** as the default (fastest to parity, no new licensing exposure), and leave sidecar bundling as a documented, opt-in build variant once the licensing question is explicitly resolved.

### 3.3 Project structure

```
orange-loaf/
├── src/                      # Svelte frontend
│   ├── lib/
│   │   ├── components/       # DaisyUI-based components (see §5.2)
│   │   ├── stores/           # runes-based state (video.svelte.ts, queue.svelte.ts, settings.svelte.ts...)
│   │   ├── ipc/              # typed wrappers around invoke()/listen() per Rust command
│   │   └── i18n/             # locale JSON, generated from legacy languages.yaml (§4.11)
│   ├── routes/                # NOT SvelteKit routes — just view components swapped by app-shell state
│   │   ├── Convert.svelte
│   │   ├── Queue.svelte
│   │   ├── History.svelte
│   │   ├── Profiles.svelte
│   │   └── Settings.svelte
│   ├── App.svelte
│   └── main.ts
├── src-tauri/                 # Rust backend
│   ├── src/
│   │   ├── probe/             # ffprobe invocation + typed serde structs (§4.4)
│   │   ├── encoders/          # one module per encoder family, `EncoderProfile` trait (§4.3, §4.6-4.8)
│   │   │   ├── common/        # generate_ffmpeg_start/filters/ending equivalents
│   │   │   ├── x265.rs, x264.rs, svt_av1.rs, av1_aom.rs, rav1e.rs, vp9.rs, vvc.rs, webp.rs, gif.rs, gifski.rs, copy.rs, modify.rs
│   │   │   ├── nvenc_ffmpeg.rs, videotoolbox.rs, vaapi.rs
│   │   │   └── rigaya/         # nvencc.rs, qsvencc.rs, vceencc.rs (shared trait impl, §1.3 family #2)
│   │   ├── hdr/                # MasterDisplay/CLL type + per-family formatters (§4.7)
│   │   ├── audio.rs, subtitles.rs, naming.rs
│   │   ├── job_runner/          # queue state machine, progress parsing (§4.5, §4.9)
│   │   ├── config.rs, profiles.rs, history.rs
│   │   ├── tools/                # binary discovery + GitHub-release installer (§1.3, §4)
│   │   └── commands.rs           # #[tauri::command] surface (§4.2)
│   └── tauri.conf.json
├── design/                     # design system + verified mockups (already built, §2)
├── notes/plan.md               # this file
└── temp/FastFlix/               # reference clone, read-only, not shipped
```

---

## 4. Backend architecture (Rust)

### 4.1 Process model

One Tauri process. No multiprocessing, no `multiprocessing.Queue`. Structure:

- A long-lived `JobRunner` (tokio task) owns `Vec<Job>` + a cursor, exactly mirroring the legacy "ordered list of `Command`s dispatched one at a time" model (§1.3) — that part of the design is good and should be kept, just without the two-process round trip.
- Encode subprocesses spawned via `tokio::process::Command`, `.current_dir(&work_dir)`, stdout/stderr captured via `Stdio::piped()` and read with `tokio::io::BufReader` + a manual `\r`-or-`\n` line splitter (rigaya tools emit `\r`-only progress updates — a plain `.lines()` won't see them; see §4.5).
- All cross-task communication is `tokio::sync::mpsc` internally; everything that reaches the UI goes out via `window.emit("<topic>", payload)`, consumed in Svelte with `@tauri-apps/api/event.listen`.
- **Fix the legacy liveness gap**: a supervisor task watches every spawned job's `JoinHandle`/exit and emits a `backend-error`/`job-crashed` event with an actual diagnostic payload (see §4.12) — the legacy app's `("error", video_uuid, command_uuid)` tuple carries zero information about *why*; don't reproduce that.

### 4.2 IPC contract (indicative — refine per milestone)

**Commands** (`invoke()`, request/response):
`probe_video(path) -> VideoInfo`, `generate_thumbnail(params) -> ThumbnailHandle`, `list_encoders() -> Vec<EncoderMeta>`, `build_preview_commands(job_spec) -> Vec<CommandPreview>`, `enqueue_job(job_spec) -> JobId`, `reorder_queue(order)`, `cancel_job(id)`, `retry_job(id)`, `pause_queue()/resume_queue()`, `pause_current_encode()/resume_current_encode()`, `load_profile(name)/save_profile(...)/delete_profile(name)`, `get_config()/set_config(...)`, `detect_tools()` (ffmpeg/ffprobe/rigaya/gifski/tesseract status, replaces "Detected External Programs"), `get_history()/apply_history_entry(id)`, `check_for_update()`.

**Events** (`emit()`, push):
`job:progress {job_id, percent, eta_secs, fps, speed, size_estimate_mb}`, `job:log {job_id, line}`, `job:status {job_id, status}` (queued/running/paused/complete/error/cancelled — **with a reason string on error**, fixing §1.3/§4.12), `queue:changed`, `backend-error {...}`, `tool-download:progress`.

### 4.3 Encoder abstraction

```rust
trait EncoderProfile {
    fn family(&self) -> EncoderFamily;             // FfmpegNative | RigayaExternal | HybridPipe
    fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep>;
}
enum EncodeStep {
    Argv(Vec<String>),                              // 29 of 30 encoders
    Piped { producer: Vec<String>, consumer: Vec<String> }, // gifski only — real OS pipe, no shell string
}
```
One struct per encoder (30 total), each `build_steps` a close translation of its `command_builder.py::build()`. The shared `generate_ffmpeg_start/filters/ending/all` helpers (§1.3) port as a `FfmpegArgBuilder` used by ~20 of the 30; the rigaya family gets its own smaller shared module (mirrors `common/encc_helpers.py`); gifski is the one deliberate exception to "always argv, never a shell string" (wire two real `Stdio::piped()` children instead of `shell=True`).

**Two-pass**: a generic helper parameterized by a per-family closure for the codec-specific "middle" fragment, collapsing the ~9 near-duplicated Python two-pass blocks into one Rust helper + small per-encoder closures. Always use **absolute** pass-log paths (never rely on process `cwd`, unlike the Python x265/SVT-AV1 code).

**Filter chain**: an explicit ordered `Vec<FilterStage>` enum with a fixed `.render()` order (§1.3's 14-step list) — callers control *presence*, never *order*. Subtitle burn-in wrapping (overlay-first vs. filters-first) is its own tested function, not an inline conditional.

**HDR formatting**: one shared `MasterDisplay`/`ContentLightLevel` struct parsed once from ffprobe JSON, with **per-family renderers** (`to_x265_string()`, `to_svtav1_string()` — with the ÷50000/÷10000 rescale unit-tested explicitly, `to_vvenc_string()`, `to_rigaya_flags()`). Do not unify into one "HDR string" — the four syntaxes are genuinely different and today's Python code is correct not to.

**Audio/subtitle track addressing**: two distinct newtypes, `FfmpegStreamIndex(u32)` (global, ffprobe-absolute) and `RigayaTypedOrdinal(u32)` (1-based, per-media-type) — never let the compiler allow mixing them (§1.3's "silently picks the wrong track" gotcha).

**Unit-safety newtypes** for settings that look alike but aren't: `Crf(f32)` vs `Qp255(u8)` (rav1e's QP is 0–255, everyone else's is roughly 0–51/0–63) vs `QpDb(u8)`.

Full per-encoder flag tables: **Appendix A**.

### 4.4 Data model translation

- `VideoInfo`/`Format`/`Stream` as real `serde` structs from ffprobe JSON (not an untyped dict) — type only the ~20 fields actually consumed (enumerated in the research): `index, id, codec_type, codec_name, width, height, pix_fmt, bits_per_raw_sample, color_primaries/transfer/space, chroma_location, r_frame_rate, avg_frame_rate, sample_aspect_ratio, bit_rate, channels, disposition, tags, side_data_list`.
- `EncoderSettings` as a `#[serde(tag = "name")]` **tagged enum**, one variant per encoder — this single change eliminates *three* separate Python workarounds found in the research: the "assign after construction, not as a constructor kwarg, or Pydantic's Union validation silently re-coerces the type" hack in `ff_queue.py::get_queue()`, the "instantiate every settings class just to find which `.name` matches" reverse-lookup (used identically in `ff_queue.py`, `profile_window.py`, and `container.py`'s history-apply), and `Profile`'s ~28 `Option<XSettings>` sibling fields (replace with one tagged field).
- `Profile`, `HistoryEntry`, `Config` port as straightforward structs; keep field names close to the legacy YAML for user-migration goodwill (a config/profile/queue/history **importer** for existing FastFlix users is a nice-to-have, see §8).

### 4.5 Progress parsing — do it properly this time

The legacy app never passes `-progress` and instead hand-splits ffmpeg's human stderr stats line on `=` (a real, working, but fragile trick — see the research for the exact algorithm if a stopgap is ever needed). **Rust port: use `-progress unix://<socket>` or `-progress pipe:1`** (with normal stats suppressed via `-nostats` and stderr captured separately) — FFmpeg's machine `key=value\n` protocol (`frame`, `fps`, `bitrate`, `out_time_us`, `speed`, terminated by a `progress=continue|end` line) parses trivially with `str::split_once('=')`, no regex needed, and is strictly more robust than string-splitting on every `=` in a human-formatted line.

Still needed for the 9 rigaya encoders: a small regex (`^\[(\d+\.?\d*)%\]`) over their bracket-percentage lines — **and remember these tools emit `\r`-only updates, not `\n`-terminated ones**; read raw bytes and split on `[\r\n]+` yourself rather than trusting a line-oriented reader that only recognizes `\n`.

Collapse the legacy's three separate stringly-typed Qt signals (`speed`, `bitrate`, `nvencc_signal`) into one typed `JobProgress { percent, eta_secs, fps, speed, size_estimate_mb }` event regardless of encoder family.

### 4.6–4.8: Filters / HDR / Two-pass

Covered in §4.3 above; see Appendix A for the exhaustive per-encoder flag reference.

### 4.9 Queue / job runner

State machine mirrors the legacy shape deliberately (it already handles 2-pass/palette-then-GIF/resume uniformly with no special-casing) — `Vec<Job>` where `Job { steps: Vec<EncodeStep>, cursor: usize, status: JobStatus }`, advanced by the `JobRunner` task. **New, on top of legacy parity**: an aggregate progress value derived from `(jobs completed, current job's percent, total jobs)` emitted as its own event — this directly powers the segmented progress bar in the Queue mockup (§2.2) and fixes the confirmed "no aggregate progress" gap.

Persist the queue as JSON (or keep YAML for human-readability/migration — see §8) with the same atomic-write pattern the legacy app uses (write to a temp file in the same directory, then rename) — that part of `ff_queue.py` is correct and worth keeping. Replace the legacy's file-lock-with-60s-staleness-heuristic with either a proper OS file lock (`fd-lock` crate) or, simpler, enforce single-instance via `tauri-plugin-single-instance` and drop the lock entirely (see §8 — this is a scope decision, not just an implementation detail).

### 4.10 Settings / profiles / history

- **Config**: one Rust struct mirroring the ~50-field `Config` schema in the research (paths to ffmpeg/ffprobe/rigaya/gifski/hdr10plus_parser/tesseract/mkvmerge, output naming template, theme, language, UI scale, queue/history behavior flags, sane-audio-codec allowlist, GPU fingerprint/capability cache, OCR settings). **Centralize config-path resolution into exactly one function** (env var → CWD `fastflix.yaml`/portable → OS data dir) — the legacy code triplicates this logic across three files with subtly different parameter lists; don't reproduce that.
- Actually **apply** the `priority` setting at job-spawn time (it's a persisted no-op in the legacy app — a one-line fix, not a redesign).
- **Profiles**: tagged-enum settings field (§4.4) removes the "which Option<X> slot matches this encoder" lookup entirely. Decide explicitly whether built-in presets (Standard / UHD HDR10 Film / 1080p Film) are read-only templates or truly editable — the legacy behavior ("edits to default-named profiles silently revert on next launch because `Config.load()` always re-injects them") is a real gotcha, not a feature; don't carry it forward silently.
- **History**: `HistoryEntry` keeps the *full* settings dump (not just a summary string) — this is what makes "apply last used settings" possible. Preserve the deliberate scope limit (reapplies encoder settings only, not source/trim/track selection) or expand it on purpose — either is fine, just make it a decision (§8).

### 4.11 i18n

Legacy `languages.yaml` is a flat `{ "<English source string>": { "<lang-code>": "<translation>" } }` map, 999 keys × 14 languages (`deu, eng, fra, ita, spa, chs, jpn, rus, por, swe, pol, ukr, kor, ron`), ~99% coverage, looked up via `t("<exact English string>")` with the English string itself as the key (no slug layer) and a silent fallback to the raw key on any miss.

Port to a standard Svelte i18n library (`svelte-i18n` or `paraglide-js`), one JSON file per locale. **Do the key migration as a scripted slug pass** (`add_to_queue` instead of the literal sentence) rather than a literal 1:1 port — using raw English sentences as keys means every future copy edit invalidates 14 translations simultaneously; a stable slug avoids that. This also fixes, for free, the legacy's "language change requires restart" limitation (a reactive `$locale` store re-renders live; there's no `lru_cache`-style staleness trap in a reactive framework the way there is in Qt widgets whose `.setText()` ran once at construction).

### 4.12 Error model

```rust
enum FastFlixError {
    MissingTool(String),
    Config(String),
    Probe(String),
    Encode { exit_code: Option<i32>, stderr_tail: String },
    Internal(String),
}
```
**Fix the legacy's biggest diagnosability gap**: its `("error", video_uuid, command_uuid)` IPC payload carries zero information about *why* — the operator must separately open a log panel to find out. The Rust `job:status` error event carries `stderr_tail`/`exit_code` directly. Keep the legacy's good instinct of treating "process exited fast with a nonzero code" and "we saw a known failure substring in stderr" as an OR condition (fast-failing processes, e.g. a VAAPI device-open failure, can exit before a log-tailing task has read a single line) — don't rely on substring-matching alone.

---

## 5. Frontend architecture

### 5.1 Views

No router/SvelteKit — five view components swapped by a single `currentView` rune in the app shell, matching the icon rail in the design (§2.3): `Convert`, `Queue`, `History`, `Profiles`, `Settings`.

### 5.2 Component inventory (legacy control → new component)

| Legacy (Qt) | New (Svelte + DaisyUI) |
|---|---|
| `SettingPanel._add_combo_box/_add_text_box/_add_check_box/_add_modes` factory (identical shape across all 30 encoder panels) | One declarative **field-schema → renderer**: a TS array of `{type, label, tooltip, options, bindKey}` per encoder, rendered through a handful of DaisyUI primitives (`select`, `input`, `checkbox`, `radio`+`join`). Adding encoder #31 becomes a data change, not a new Svelte file. |
| CRF-vs-Bitrate radio group + custom fields | DaisyUI `join` of two toggle buttons, each conditionally revealing a `select`+`input` ("Custom") sub-form |
| Per-track rows (Audio/Subtitle/Data/Queue) | One generic `<TrackRow>` component, slot-parameterized; reorder via native HTML5 drag-and-drop or up/down icon buttons; enable/preserve via `toggle` |
| Tab-visibility-per-encoder (`setTabVisible`/`setTabEnabled`) | A `$derived` store keyed off the selected encoder's capability flags, gating `{#if}` around each accordion section |
| Source Details / Raw Commands / Debug read-only YAML dumps | `mockup-code` or a syntax-highlighted `<pre>`, generated client-side from the same settings object — one component, not five near-duplicate read-only browsers |
| Visual Crop window (`crop_window.py`) — mouse-drag edges/corners over a rendered frame, unrotated↔rotated coordinate transforms | Absolutely-positioned drag-handle `<div>`s over an `<img>`/canvas; port the coordinate-transform math verbatim (pure, encoder-agnostic) into a shared TS util |
| Concat / Multiple-Files windows (both: folder scan → probe → reorderable table → commit) | One generic `<MediaFileTable>` (DaisyUI `table` + native drag-and-drop), parameterized by the commit action |
| Disposition popup | A small DaisyUI `dropdown` attached to each track row instead of a separate floating window |
| Settings' 4 tabs, "Detected External Programs" list | DaisyUI `tabs` + `form-control` rows; tool status → a `list` of `badge` (success/error), generated from one backend `detect_tools()` call |
| Queue row actions (reload/cancel/retry/watch/open-dir) | DaisyUI `dropdown` "kebab" menu instead of 5 always-visible icon buttons |
| "Encoding Status" tab (global log + stats) | A per-row `<JobLogPanel>` behind each Queue row's disclosure chevron (built, see updated Queue mockup) — stats header (Time Left/Elapsed/Size Estimate) + `Hide NAL unit messages` toggle + a scrollable mono terminal block fed by that job's `job:log` events (§4.2); see §2.4 |
| Profile audio pattern-match rule builder | DaisyUI `table`/repeated `card` list, match-item-dependent input swap via `{#if}` |

Full literal inventory to check off against: **Appendix B**.

### 5.3 State

Runes-based stores (`.svelte.ts` files, no separate state library needed — Svelte 5 runes work identically in plain `.ts` modules):
- `videoStore` — currently loaded source + probe result + settings-in-progress (mirrors legacy `FastFlix.current_video`)
- `queueStore` — job list + aggregate progress, updated by `job:*` events
- `settingsStore` / `profilesStore` / `historyStore`
- Command preview = a **pure `$derived`** off `videoStore`, matching the legacy's actual behavior (no debounce needed there — it's cheap string building); thumbnail fetch is the one thing that should be debounced/hash-gated (150–300ms), mirroring the legacy's `last_thumb_hash` optimization, since it's the one genuinely expensive op in the chain.

### 5.4 DaisyUI theme

Implement the token table in §2.1 as a DaisyUI 5 custom theme (`data-theme="orangeloaf"`) using its new CSS-custom-property theming engine — this is a direct upgrade over the legacy's restart-required QSS swap; theme changes become instant. Carry forward the legacy's 3 theme names (dark/light/onyx) as a *starting reference palette* only if a light theme is wanted at all (§8) — the approved design direction (§2) is dark-only, matching the anchor style's `Theme Mode: Dark Mode (enforced)`.

---

## 6. Step-by-step implementation plan

Each milestone lists goal → key tasks → primary files/modules → acceptance criteria. Sequenced so there's a **real, working, end-to-end encode as early as M3** — everything after that is breadth (more encoders, more panels), not depth.

### M0 — Bootstrap
- `bun create tauri-app` (Svelte-TS template), strip to plain Svelte + Vite (no SvelteKit).
- Add Tailwind 4 + DaisyUI 5, wire the `orangeloaf` theme from §5.4.
- Set up Rust workspace layout from §3.3; add `tokio`, `serde`, `serde_json`, `serde_yaml`, `thiserror`.
- Add `tauri-plugin-shell`, `-dialog`, `-fs`, `-window-state`.
- **Acceptance**: empty app launches, shows the icon rail shell with all 5 nav destinations (inert), dark theme matches the token table exactly (spot-check against the mockup PNGs).

### M1 — Config & tool discovery
- Port `Config` schema (§4.10) as a Rust struct; implement the single config-path-resolution function (env → CWD → OS data dir), matching legacy precedence for user migration goodwill.
- Port binary discovery for ffmpeg/ffprobe (env → CWD → PATH → known download dir) — no auto-download yet, just detection + a Settings screen showing status (parity with "Detected External Programs").
- **Acceptance**: Settings view shows real green/red status for ffmpeg/ffprobe on the dev machine; manually pointing at a binary via the dialog plugin persists across restart.

### M2 — Media probing + preview
- `probe_video` command: shell `ffprobe`, parse into typed structs (§4.4), including HDR10 static-metadata extraction (§1.2's rescale formula) and HDR10+ ffprobe-fallback detection.
- Thumbnail generation: single-frame ffmpeg extraction, hash-gated on {crop, scale, time, track, rotate, flip, hdr} exactly like legacy's `last_thumb_hash`.
- Convert screen skeleton from the mockup: header (source path, Change Source), preview `<img>` + scrubber (regenerate on slider-release, not continuous).
- **Acceptance**: load a real video file, see correct probed metadata (resolution, codec, HDR10 badge if applicable), scrub the slider and see the thumbnail update.

### M3 — First real encode (x265 end-to-end)
- Implement the `EncoderProfile` trait + `x265` struct (§4.3) — the richest encoder, deliberately first so every later encoder is a subset of this work.
- Filter chain builder (§4.3) with crop/scale/rotate/flip (deinterlace/denoise/eq can lag to M6).
- `-progress`-based progress parsing (§4.5) end to end: spawn real ffmpeg, stream `job:progress` events, render in a minimal status view.
- Command preview panel (derived, live, matching legacy's un-debounced synchronous rebuild).
- **Acceptance**: pick a source file, adjust CRF/preset in a real (even if unstyled) Quality form, hit Convert, watch a real progress bar move, get a real output file. This is the "does the whole pipe actually work" milestone — everything after is breadth.

### M4 — Remaining ffmpeg-native software encoders
- x264, av1_aom, svt_av1(+avif), rav1e, vp9, vvc, webp — each a small `EncoderProfile` impl reusing the M3 filter/two-pass/HDR infrastructure. Per-encoder Quality accordion sections via the field-schema renderer (§5.2).
- Two-pass (§4.3) wired for the encoders that support it; the discardable pass-1-container table (mp4/matroska/webm per codec) implemented as explicit data, not a shared constant.
- **Acceptance**: every software encoder in Appendix A's table produces a valid output file for a representative test clip; 2-pass bitrate mode verified for at least x265, x264, SVT-AV1.

### M5 — Hardware encoders
- ffmpeg-native hardware first (VideoToolbox ×2, VAAPI ×4, ffmpeg-NVENC ×2) — still "family #1," just hardware-accelerated.
- Then the rigaya external-exe family (§1.3 family #2): shared trait impl for nvencc/qsvencc/vceencc × hevc/avc/av1 (9 structs), native VPP flags (crop order L,T,R,B!), `--avhw`/`--avsw` reader-mode selection, GPU/capability probing (`--check-features` parsing — Windows-only parity to start, see §8 for cross-platform scope).
- Rigaya audio/subtitle addressing using the `RigayaTypedOrdinal` newtype (§4.3) — test explicitly against a source with mixed audio+subtitle+data streams to catch the off-by-one class of bug called out in the research.
- **Acceptance**: at least one hardware path validated per platform available in dev (e.g. VideoToolbox on macOS); rigaya family validated against whatever GPU is available, else covered by unit tests on argv construction alone.

### M6 — Filters, crop tool, trim
- Full filter chain: deinterlace, denoise, deblock, eq, HDR tonemap (opencl/vaapi/software zscale variants).
- Visual crop tool (drag handles, rotated↔unrotated coordinate transform ported verbatim from `crop_window.py`).
- Start/End time trim with fast-seek vs. exact-seek modes.
- **Acceptance**: crop-then-encode produces correctly-cropped output at the exact pixel rect shown in the tool; trim produces correct duration.

**Implementation notes (scope decisions made while building this milestone):**
- **Shipped**: deinterlace, crop (with a real drag-to-select/resize visual tool, 8 handles, letterbox/pillarbox-aware), scale, rotate, h/v flip, and start/end trim with a fast-seek/exact-seek toggle — all wired end to end (Rust `FilterSpec`/`Trim` → `EncodeContext` → argv → Convert screen's Filters accordion).
- **Deferred, not built**: denoise, deblock, eq, and HDR tonemap. These need their own field-schema and, for tonemap, real HDR test material to verify against — the same "don't ship an unverified guess" bar applied to M5's hardware-encoder scoping. Left for a follow-up pass rather than folded silently into this one.
- **No rotated↔unrotated coordinate transform was needed.** The plan assumed one because the legacy tool's *preview* rendered live rotation/flip, so crop coordinates had to be mapped back through that transform. This app's preview thumbnail is always the raw, unrotated source frame (`generate_thumbnail` takes no filter arguments), and the filter chain applies crop *before* rotate (§9 Appendix A's fixed stage order) — so the crop tool's drag coordinates already are pre-rotation source-pixel coordinates, with no transform in between. Simpler by construction, not a missing feature; revisit only if a live-rotated preview is added later.
- **Trim lives inside the Filters accordion**, not as its own section — this milestone's own title bundles them, and there wasn't a strong IA reason to split them into separate accordion entries.
- **Crop values are snapped to even pixel offsets and dimensions** (both drag-tool output and hand-typed numeric input) — not called out explicitly in this plan, but required for correctness: ffmpeg errors on an odd final width/height for 4:2:0 output (which is what every shipped encoder here produces), and an odd crop *offset* shifts chroma siting by half a pixel even when it doesn't error.
- **Found and fixed a real reactivity bug while wiring this up, unrelated to filters/crop/trim specifically**: the Convert screen's live command-preview `$effect` read `encode.settings`/`encode.filters` by reference only, which (verified directly against this project's Svelte 5 version with a throwaway repro, not assumed from docs) does *not* track mutations to their nested fields — meaning CRF/preset/tune slider edits were never actually refreshing the live preview. Fixed by reading through `$state.snapshot(...)` instead, which both forces the correct deep dependency and is the documented way to hand reactive state to a non-Svelte API (Tauri's `invoke`).

### M7 — Audio / Subtitles / Data / Cover panels
- Track selection UI (`<TrackRow>` generic component, §5.2) for all four track types.
- Audio: codec conversion + quality/bitrate tables (§Appendix A) + downmix + disposition popup.
- Subtitles: copy/burn-in (picture vs. text ordering rule from §1.3), external file attach, disposition.
- Data/Cover: preserve/extract flows, container-compatibility graying (e.g. fonts into `.mp4`).
- PGS→SRT OCR: **explicitly scoped as a separate, optional feature module** (Tesseract/mkvmerge discovery + a pre-encode action), not folded into the encode pipeline — matches the legacy's own architectural boundary (§1.3/Appendix A note).
- **Acceptance**: multi-track source (2+ audio, 2+ subtitle, embedded cover) round-trips correctly per the legacy's exact track-addressing rules for both ffmpeg-native and rigaya families.

**Implementation notes (scope decisions made while building this milestone):**
- **Shipped**: real audio/subtitle track probing (type-relative indices matching ffmpeg's own `-map 0:a:{index}` addressing, not ffprobe's global stream index); per-track audio enable/disable + copy-or-convert(codec, quality 0–9, optional downmix); per-track subtitle enable/disable + copy; a generic `<TrackRow>` component (§5.2) shared by both panels. Verified against a real multi-track fixture (`tests/fixtures/multitrack.mkv`, 1 video + 2 audio + 2 subtitle) with a real ffmpeg run + ffprobe assertions on the *output*, not just argv inspection — this is what caught (rather, confirmed correct) the output-ordinal remapping: ffmpeg's per-stream options (`-c:a:N`) address output stream position, not input index, so disabling track 0 and keeping track 1 must still land on `-c:a:0`.
- **Deferred, not built**: subtitle burn-in (needs picture-vs-text filter-chain ordering per §1.3 and its own fixture/verification — explicitly the plan's own "most important piece of business logic in the filter system," not something to rush), the disposition popup, Data/Cover panels, external subtitle file attach, drag-to-reorder tracks, and PGS→SRT OCR (already scoped out by the plan itself as a separate module). The rigaya encoder family doesn't exist yet at all (deferred since M5), so the acceptance criterion's rigaya half is necessarily out of scope until that family lands.
- **Codec choice list is scoped to what's verified on this dev machine's ffmpeg build**: aac, libopus, libmp3lame, ac3, flac. Appendix A's full table also lists libvorbis and libfdk_aac, neither of which this build has compiled in (`libvorbis` specifically — the native, non-`lib` `vorbis` encoder exists but is a different encoder); offering codecs that silently fail at encode time would be worse than a shorter, honest list.
- **`TrueHD` is treated as lossless** (no bitrate flag), not under the ac3/eac3 bitrate formula — Appendix A's condensed table lists it in both places, which is contradictory; lossless is the technically correct reading for a format that's fundamentally Dolby's lossless codec.
- **Empty track selection still defaults to "copy everything"** for both audio (unchanged behavior) and subtitles (a new, deliberate default — the pre-M7 code silently dropped all subtitles, which was a real gap, not an intentional cut).

### M8 — HDR10 / HDR10+ / Dolby Vision pipeline
- Per-family HDR string formatters (§4.3/§4.7) with the SVT-AV1 rescale unit-tested against known-good values.
- HDR10+ JSON extraction (ffmpeg | hdr10plus_tool pipe) as an explicit pre-encode pipeline step, kept structurally separate from the main encode steps (mirrors legacy's widgets-layer-thread boundary).
- Verify the real `hdr10plus_tool`/`hdr10plus_parser` CLI grammar once and use one consistent argv order (the legacy source has two different, inconsistent orderings — don't copy either blindly).
- Dolby Vision: pass-through flags only for rigaya encoders, no DV parsing in Rust at all (matches legacy exactly — there is no FastFlix-side DV logic to port).
- **Acceptance**: an HDR10 source retains correct mastering-display/CLL in x265 and at least one rigaya output, verified with `ffprobe -show_frames`/`mediainfo` on the result.

**Implementation notes (scope decisions made while building this milestone):**
- **Shipped**: real HDR10 static-metadata extraction (`probe_video` now returns full mastering-display + content-light values, not just an `hdr10: bool`) and per-family injection for x265 and SVT-AV1, both verified with a genuine round trip — encode the real `tests/fixtures/hdr10.mkv` fixture, then re-probe the *output* and confirm the values survived — not just that the formatted argv string looks plausible. Preservation is automatic (no "keep HDR10" toggle exists; any source that has it gets it re-injected on encode) since there's no competing default to choose between yet.
- **The two target formats are genuinely different scales, confirmed empirically rather than assumed from Appendix A's prose**: x265's `master-display=` wants integers already in the HEVC SEI's own fixed-point base (÷50000 chromaticity, ÷10000 luminance — verified by encoding a fixture with hand-chosen values and reading ffprobe's exact JSON back); SVT-AV1's `mastering-display=` wants plain decimal chromaticity (0..1) and nits luminance — a real "inverse rescale" from the HEVC scale, exactly the bug-risk §1.2 called out. AV1's own on-disk representation (confirmed via ffprobe) uses yet another base (÷65536) internally, which is why the SVT-AV1 round-trip test uses an epsilon tolerance instead of exact equality — a real, small, expected precision loss from that format's own resolution, not a bug.
- **This installed SVT-AV1 build (v4.2.0) rejects an `enable-hdr` flag** the plan's prior-art source assumed existed ("Error parsing option enable-hdr"). Verified that `mastering-display=`/`content-light=` alone, with no enable flag at all, is sufficient to emit the HDR OBUs — so that's what's implemented. This may be a version-specific SVT-AV1 CLI change; worth re-checking if a future SVT-AV1 upgrade reintroduces the flag.
- **Deferred, not built**: HDR10+ dynamic metadata (extraction and injection both need `hdr10plus_tool`/`hdr10plus_parser`, which aren't installed on this dev machine — implementing against an unverifiable CLI grammar would repeat exactly the mistake §1.2 warns about, copying an inconsistent argv order blindly). Dolby Vision stays fully out of scope, as planned — it's rigaya-only, and the rigaya encoder family doesn't exist yet (deferred since M5). x264/VideoToolbox don't get HDR10 injection: x264 is 8-bit-only in this app (no HDR-capable pixel format), and VideoToolbox's ffmpeg wrapper only supports "color-details" tagging per Appendix A's own table, a smaller and different feature than the full mastering-display SEI injection x265/SVT-AV1 get.

### M9 — Queue system
- `JobRunner` state machine (§4.9), sequential dispatch, pause-queue vs. pause-current-encode as genuinely separate concepts (with the legacy's own warning text about VCE/NVENC not supporting process-suspend preserved verbatim — it's load-bearing user guidance).
- **New**: aggregate progress bar (segmented, per the Queue mockup) — this is net-new, not parity.
- Queue persistence (atomic write, JSON or YAML — §8) + startup recovery (drop completed, reset stuck "running" state left over from a crash, prompt to keep in-progress items).
- **Acceptance**: queue 5+ mixed-encoder jobs, reorder before they start, cancel one mid-run, verify the aggregate bar and per-row states all stay correct; kill and restart the app mid-queue, verify recovery prompt and state.

**Implementation notes (scope decisions made while building this milestone):**
- **Shipped**: a real `Job`/`Queue` model (JSON, atomic write via temp-file-then-rename) with startup recovery (any `Running` job left over from a crash resets to `Queued` unconditionally); sequential dispatch that automatically chains to the next queued job on completion/failure/cancellation; real cancellation (kills the actual ffmpeg child process via a `tokio::select!` race against a oneshot channel, not just a status flag); pause/resume at the queue level; reorder via Move Up/Down; retry (re-enqueues a failed/cancelled job's exact settings); a segmented aggregate progress bar; per-row disclosure with a *live* log panel fed by a new `job:log` event (the M2.4 design decision from earlier in this plan, now actually built) plus Time-Left/Elapsed stats computed from ffmpeg's own reported `speed` rather than a separately-tracked wall-clock timestamp. All of the above — including real cancellation specifically, the trickiest part — is covered by tests that exercise the actual runner against real ffmpeg processes (`tests/fixtures/`-based, using `tauri::test::mock_app` + a generic `AppHandle<R: Runtime>` on the internal runner functions so the same code path used in production is what the tests actually call), not mocked-out approximations of it.
- **Pause-queue and pause-*current-encode* are the two genuinely separate concepts the plan calls for, but only the first is implemented.** Suspending an in-flight ffmpeg process (SIGSTOP/CTRL_BREAK-equivalent) is a real, separate capability with its own platform/encoder-family caveats — moot to build correctly right now anyway, since none of the hardware encoder families where the legacy app's warning actually applies (VCE/NVENC) exist in this codebase yet. Documented as deferred, not silently dropped.
- **Reordering is Move Up/Down, not drag-and-drop.** Functionally equivalent for the acceptance criterion ("reorder before they start"); native HTML5 drag-and-drop has its own accessibility/interaction-design surface that didn't seem worth taking on in the same pass as everything else here.
- **Crash recovery resets stuck `Running` jobs to `Queued` unconditionally** rather than the plan's fancier "prompt to keep in-progress items" — an interactive recovery dialog is a real, separate piece of UI/UX design (what does the prompt say, what are the exact choices, does it block startup) that deserves its own decision rather than being rushed in as a side effect of building the runner.
- **No file-size tracking or estimate.** ffmpeg's `-progress` output does carry a `total_size` field, but nothing here parses or surfaces it (the mockup's "Size Estimate" stat is not implemented) — this would need extending `EncodeProgress` for a single cosmetic stat, deferred rather than scope-creeping this milestone further.
- **A finished job's log is only whatever happened to still be in the frontend's in-memory buffer when it finished** (capped at 500 lines) — nothing durably persists a job's full log. This is an honest limitation, not a bug: Failed jobs still show their actual error message (that part *is* persisted, as `JobStatus::Failed`'s `message` field), just not a full scrollback.

### M10 — Profiles
- Tagged-enum settings persistence (§4.4/§4.10) — no reverse-lookup-by-name needed.
- Decide + implement rename/duplicate/edit scope (§8) rather than silently porting create-only.
- Audio pattern-match rule builder (match-type/match-item/conversion/downmix/title-mode), ported from `profile_window.py`'s `AudioMatch` model.
- **Acceptance**: save a profile, edit it (per whatever scope was decided), apply it to a freshly loaded video and confirm encoder settings + audio/subtitle auto-selection rules both apply correctly.

**Implementation notes (scope decisions made while building this milestone):**
- **§8's open decision is resolved: profiles are fully editable** — rename (inline), duplicate, delete, and re-save encoder settings/filters/audio rules over an existing profile by id, including the three built-in starting presets. Nothing re-injects or reverts a built-in profile's edits on load, unlike the prior art's `Config.load()` behavior that the plan explicitly calls out as "a real gotcha, not a feature."
- **Audio auto-selection rules are a real, clean-room-designed subset of the prior art's `AudioMatch` model** (studied from FastFlix's `profiles.py`/`profile_window.py` for behavioral reference only, per this project's clean-room policy — no code or structure copied). Shipped: match by All/Language/Channels, apply to All-matches or First-match-only, with a Copy-or-Convert(codec/quality/downmix) action reusing M7's `AudioAction` type directly — this is what makes the implementation genuinely simpler than the source app's, which instead needs ~25 sibling `Optional<XSettings>` fields and a handful of `field_validator`s to normalize legacy/loosely-typed inputs. Rules evaluate in list order with last-match-wins per track, which a real test (`a_later_rule_overrides_an_earlier_catch_all_rule_for_the_same_track`) pins.
- **Deferred, not built**: Title-substring and explicit-Track-index matching, and the `Last` match-type (keeping only All/Language/Channels × All/First covers the actual common cases — "keep tracks in these languages," "handle a specific channel layout differently" — without reproducing every knob the research turned up). Subtitle auto-selection rules aren't implemented at all: notably, the prior art's own `Profile` model has its `subtitle_filters` field commented out and unused, so this isn't a gap relative to a working reference feature, just an unfinished one in the source app too. `AdvancedOptions` (deblock/denoise/tone-map/color-space overrides, etc.) isn't part of the `Profile` model here because none of those controls exist anywhere in this app's pipeline yet, for any encoder — a profile can't carry a setting this codebase has no other way to apply.
- **A real bug was found and fixed while building this milestone's audio-rule action editor, in already-shipped M7 code**: the Convert screen's per-track downmix `<select>` compared its bound value (a `number`) against `<option>` elements' values (always strings in a template), which never matches — the dropdown silently rendered with nothing selected whenever a non-default downmix was actually in effect. Caught via the same visual-screenshot verification this session uses throughout (a seeded profile showing "Stereo" chosen made the blank select obvious), not by code review — fixed in both places by stringifying the compared value.

### M11 — History
- `HistoryEntry` with full settings dump (not just a summary) + thumbnail-from-source-at-trim-point generation.
- "Apply last used settings" / History view "Apply" button, matching the legacy's deliberate scope (encoder settings only, not source/trim/tracks) unless §8 decides to expand it.
- **Acceptance**: complete an encode, confirm a history entry appears with correct summary + thumbnail; apply it to a new video and confirm only encoder settings transfer.

**Implementation notes (scope decisions made while building this milestone):**
- **§8's open decision is resolved: kept the legacy's "encoder settings only" apply scope**, not expanded to a full job clone. Reasoning: trim points and audio/subtitle track-index selections are source-specific data — a trim range valid for a 2-hour recording is nonsensical replayed against a 5-minute clip, and "track index 2" can name a completely different language on a video with a different track layout. `Profile`'s audio *rules* sidestep this by re-evaluating against the new source's freshly-probed tracks; a `HistoryEntry` has no rules to re-evaluate, only a frozen snapshot of what was selected last time, so those fields just don't transfer. `settings` + `filters` are the only fields safe to replay onto an arbitrary different source.
- **Recording is queue-only, not wired to the direct (non-queued) Convert-screen encode path.** Both paths already funnel through the same `job.rs` runner, but only `queue.rs`'s `finish_job` calls `history::record`. This was a deliberate scope cut rather than an oversight: the direct-encode path has no `Job` record to source `source`/`output`/`settings`/`filters` from without threading several new parameters through `commands::start_encode` purely to satisfy history, and the app already steers real usage toward the queue (the Convert screen's primary action is "Add to Queue," per M9). Revisit if direct-encode usage turns out to be more than an edge case.
- **Only `Completed` and `Failed` outcomes are recorded — `Cancelled` deliberately is not.** A cancelled job reflects a user's own change of mind, not a result worth resurfacing later for "apply these settings again"; recording it would clutter history with entries that have no meaningful output to point to.
- **Thumbnail generation is best-effort and never blocks recording.** `history::record` tries to generate a thumbnail at the job's trim start (falling back to `0.0` for untrimmed jobs) but stores `None` on any failure (source moved/deleted, ffmpeg unavailable, etc.) rather than failing the whole history write — a history entry with a missing thumbnail is far more useful than a silently-dropped entry.
- **200-entry cap, oldest-evicted**, enforced server-side in `History::save`/`record` (`MAX_ENTRIES`) rather than exposed as a user-facing "max items" setting. The Appendix B legacy-UI inventory (§8.15) lists a max-items *selector* as a control the old History window had; a fixed server-side cap was chosen instead because it already fully addresses the actual concern (unbounded growth of a JSON file loaded on every launch) without adding a settings knob whose value users have no real basis for choosing. Revisit if real usage shows 200 is the wrong number.
- **No separate "Details" view** — folded into the same inline-disclosure row pattern already shipped for Queue (M9) and Profiles (M10) rather than a fourth, subtly-different expand/collapse idiom. Expanding a row shows the full source/output paths and (for failed entries) the complete error message.
- **A genuine test-synchronization race was found and fixed while extending the real end-to-end queue test for history assertions** (not a production bug — verified: `finish_job` always records history synchronously, before returning, holding no lock across the record call that anything else waits on). The test itself polled `state.queue` to confirm both jobs had reached a terminal status, then immediately asserted on `state.history` — two different mutexes with no ordering guarantee visible to an external thread between "queue says done" and "history has been written." Fixed by giving the test its own short polling loop against `state.history` directly before asserting on its contents. Confirmed fixed by running the full suite several consecutive times.

### M12 — Settings, GPU detection, tool auto-download
- Full Settings view per §5.2's table (paths, naming template + variable chips, audio-encoder curation list, language/theme, UI density, update-check toggle, logging level).
- GPU detection + rigaya/ffmpeg/hdr10plus_tool/7-zip auto-download (§1.3) — **Windows-first, matching legacy scope**; explicitly decide whether to invest in Linux/macOS parity here (§8) since the source app never did.
- Output naming: full pre/post-encode template variable system (§1.3), including the placeholder-token-in-filename mechanism for post-encode stats.
- **Acceptance**: naming template preview matches legacy behavior for every variable in Appendix A's naming table; tool auto-download successfully installs at least one rigaya encoder on a clean Windows VM.

**Implementation notes (scope decisions made while building this milestone):**
- **GPU detection + tool auto-download is deliberately not built at all, and the acceptance criterion's Windows-VM half is not verifiable here.** This dev machine is macOS with no Windows environment available, and the feature is inherently Windows-shaped: GPU vendor detection goes through `wmi` in the source app, and every auto-downloadable artifact (rigaya's 3 encoder exes, hdr10plus_tool, 7-Zip itself for extraction) is a Windows binary. Implementing this against an unverifiable target would repeat the exact mistake §1.2's own research warns against (copying argv/API shape blindly, never having run it) — same reasoning already applied to VAAPI/NVENC (M5) and HDR10+ (M8). §8 item 4 (cross-platform parity) is answered by this gap staying open: no investment made either way, left for whoever next has a Windows machine to build and verify it on.
- **The naming template system — both phases — is real, fully wired, and verified end to end**, not just implemented as isolated pure functions. `naming.rs` resolves all 22 pre-encode variables (Appendix A's table) from the source path, encoder settings, and probed `VideoInfo`; any post-encode variable present resolves to a literal placeholder token (`FFFSIZE`, ...) baked into the filename ffmpeg actually writes to, exactly matching the legacy "placeholder-then-rename" trick §1.3 calls out as worth keeping. Once a queued job completes, `queue::dispatch_next` probes the real output file (size + ffprobe'd bitrates) and renames it for real — verified with a real ffmpeg encode through the actual `enqueue_job` → `dispatch_next` path (`real_enqueue_resolves_naming_template_pre_and_post_encode`), not just unit tests of the string substitution. The direct (non-queued) Convert-screen path gets the same treatment via a shared `naming::finalize_output_naming` helper, since unlike M11's history recording, this one's cheap to wire into both call sites identically.
- **`VideoInfo` grew three new probe fields** (`bitrateBps`, `colorSpace`, `colorTransfer` — `colorPrimaries` was already probed internally, now also exposed) purely to give the naming template real data for `{source_bitrate}`/`{color_space}`/`{hdr}`. `hdr10: Option<Hdr10Metadata>` parameters across `start_encode`/`preview_command`/`enqueue_job` were replaced with `video_info: Option<VideoInfo>` (deriving `hdr10` from it internally) rather than adding a second parallel parameter — the frontend already held the full `VideoInfo` at every call site, so this removes a redundancy instead of adding one.
- **`{audio_codec}`/`{audio_channels}` read the *source*'s first probed audio track**, not whatever conversion codec/downmix is currently configured for it. This is a clean-room interpretation filling a real gap: the prior art's own naming variables carry the same names but its behavioral source (`video.streams.audio[0]`) is *also* the raw probed track, not the user's chosen output codec — so this isn't a deviation, it's matching what those variables have always actually meant.
- **`{bitrate}` (a pre-encode *target* bitrate) always resolves to `N-A`.** No encoder in this codebase has a bitrate-mode setting — every one is CRF/quality-only (M4's scope) — so there is no value to source this from yet, the same "can't reference a setting that doesn't exist" situation M10 hit with `AdvancedOptions`.
- **Datetimes in naming variables are UTC, not local time.** The `time` crate's local-offset lookup carries a documented soundness caveat (unsound to call in a multi-threaded process on some platforms) that isn't worth taking on for a filename timestamp — a real, deliberate, documented deviation from the prior art's naive local time, not an oversight.
- **A second genuine bug was found and fixed while writing this milestone's own real end-to-end test**: `Queue`/`Profiles`/`History`'s atomic-save helper (`save_to`) used a *fixed* temp filename (`<name>.json.tmp`) before renaming into place. In production this is safe (every real caller already holds `state.queue`'s mutex across the call), but concurrent tests using independent in-memory `Queue`s that all resolve to the same real on-disk path (none of them override `ORANGELOAF_QUEUE`) could race: one writer's rename can consume another's temp file mid-write, failing with a spurious "not found." Caught when the new naming-template test occasionally failed only under the full parallel suite, never in isolation. Fixed by giving the temp file a random per-call suffix in all three modules (queue/profiles/history all shared the identical pattern) — eliminates the race class outright rather than papering over one occurrence of it.
- **Audio-encoder curation (`disabledAudioCodecs`) is wired into both consumers, not just persisted inertly**: the Convert screen's and Profiles' audio-conversion codec `<select>` both now filter through `config.availableAudioCodecs()`, which still always includes whatever codec is *already* selected even if it's since been disabled — otherwise disabling a codec out from under an existing selection would reproduce the exact "bound value has no matching option" blank-`<select>` bug M10 found and fixed for downmix.
- **UI density is a real, visually-verified effect** (root `font-size` swap scaling every `rem`-based Tailwind size at once — see `app.css`), not an inert preference; **language and update-check are real persisted preferences with deliberately nothing behind them yet** (retranslation is M13's job; no update-checker exists anywhere in this app); **log level is persisted only, with no logging framework yet to honor it** — the same category of gap as `AdvancedOptions`, kept as a real control since §5.2 calls for one.

### M13 — i18n
- Slug-migration script from `languages.yaml` → per-locale JSON (§4.11).
- Wire `svelte-i18n`/`paraglide-js`, confirm live language switching (no restart).
- **Acceptance**: switch language in Settings, confirm the whole visible UI retranslates without a restart — an explicit improvement over legacy, verify it actually works rather than assuming.

**Implementation notes (scope decisions made while building this milestone):**
- **`@inlang/paraglide-js` chosen over `svelte-i18n`** — compiler-first (messages become tree-shakeable typed ESM functions, not a runtime dictionary lookup), which fits "bleeding-edge stack" better than a traditional runtime i18n store. No `languages.yaml` migration script was needed: this is a from-scratch (not ported) app with zero existing translated strings, so message keys were authored directly rather than migrated from anything.
- **A real, non-obvious gotcha, found and solved rather than assumed away**: paraglide's own `setLocale()` defaults to a full document reload/navigation (it's built around SSR/URL-routed apps first). Its documented `{ reload: false }` escape hatch exists for exactly this app's shape (a single-window client-rendered surface with no URL routing) but, by its own docs, "does not re-render the UI or update the document" — message functions (`m.xxx()`) call paraglide's `getLocale()` internally at call time and are plain functions, not Svelte-reactive. `src/lib/i18n.svelte.ts` closes that gap: a real `$state` rune mirrors the active locale, and `App.svelte` wraps the whole shell (`IconRail` + the current view) in `{#key i18n.locale}`, so Svelte remounts — and therefore re-evaluates every `m.xxx()` call in — that subtree on a switch. Verified empirically, not assumed: a scratch harness drove a real `<select>` change event end to end and confirmed (a) every string on screen actually changed, across three different scripts (Spanish, English, Japanese), and (b) no reload occurred, via a `window` marker that only a real navigation would clear.
- **Full-app string coverage is deliberately not done.** Every one of the 7 already-built views (Convert, Queue, History, Profiles, plus Settings) and their shared components has hardcoded English strings; this milestone extracted and translated only `SettingsView` (where the language picker itself lives) and `IconRail` (the always-visible shell chrome) into 5 real locales (en/es/fr/de/ja). This is the largest remaining mechanical gap in the app: hundreds of strings across dozens of files, none of it hard, all of it bulk work that deserves its own careful pass rather than a rushed sweep bolted onto proving the switching mechanism works. The acceptance criterion is satisfied for the scope actually claimed — switching language while on the Settings screen retranslates *everything currently visible* (Settings' own content plus the persistent nav rail) live, with no restart — rather than being claimed for the whole app when it isn't true yet.
- **Message catalogs live in `messages/*.json` + `project.inlang/settings.json`** (both tracked; inlang's own generated `project.inlang/{cache,README.md,.meta.json}` are gitignored by an inlang-managed nested `.gitignore`, left as-is). `src/paraglide/` (the compiled runtime + message functions) is generated on every build via the Vite plugin and gitignored, same treatment as `dist/`.
- **Locale detection strategy is `["globalVariable", "baseLocale"]`**, not `cookie`/`url` — there's no URL routing at all in this single-window Tauri app (§5.1's own "no router/SvelteKit" decision), and the persisted choice already has a real home (`Config.language`, M12) that `App.svelte` applies on startup, so paraglide doesn't need its own separate persistence (`localStorage`/cookie) layered on top.
- **Language *names* in the picker (`English`/`Español`/`Français`/`Deutsch`/`日本語`) are deliberately not run through `m.*`** — a language's own name in its own script is conventionally left untranslated in language pickers (a French UI still lists "English", not "Anglais"); these are identity labels, not UI copy.

### M14 — Packaging & distribution
- Resolve the ffmpeg-bundling decision (§3.2/§8) and implement accordingly.
- ~~Tauri bundler config for Windows (NSIS/WiX)/macOS (.app)/Linux (AppImage/deb)~~ **Linux-only by explicit user decision (2026-07-29, superseding this milestone's original 3-platform scope)** — parity feature set (Start Menu shortcut, uninstaller — N/A now) plus whatever's newly in scope (codesigning — resolved as explicitly out of scope, see below).
- **Acceptance**: ~~signed or explicitly-unsigned-and-documented installers produced for all 3 platforms via CI; fresh-machine install → launch → detect-ffmpeg → encode smoke test passes on each~~ **unsigned .deb/.rpm/.AppImage produced for Linux via CI; Windows/macOS are explicitly not targeted — see the Linux-only note below.**

**Implementation notes (scope decisions made while building this milestone):**
- **§8 item 1 (ffmpeg bundling) is resolved: BYO, matching the legacy app**, not a bundled sidecar. This isn't a default/omission — it's the same real reasoning §1.5 already documents (statically bundling GPL-licensed x264/x265 binaries into an MIT-licensed installer is a genuine licensing question the legacy app deliberately avoided by never doing it) plus consistency with everything already built: the entire M1 tool-discovery flow and M12 Settings screen exist specifically to detect-or-let-the-user-point-at a local FFmpeg install. `tauri.conf.json`'s `bundle` config has no `externalBin`/sidecar entry for ffmpeg/ffprobe, and nothing downloads them — this is the concrete implementation of that decision, not an oversight.
- **§8 item 6 (codesigning) is resolved: unsigned, explicitly documented**, not silently shipped as a gap. Real codesigning needs real Apple/Windows developer credentials this environment (and this repo, so far) has none of — fabricating signing config without real certificates would be worse than not having it. `.github/workflows/release.yml` is written to accept real secrets (`APPLE_CERTIFICATE`/`APPLE_TEAM_ID`/etc., `WINDOWS_CERTIFICATE`/etc.) the moment they're added as repo secrets, with comments explaining exactly what's missing — matching the legacy app's own actual shipping state (§1.5: "an unsigned/non-notarized app is the actual current shipping state, not a rewrite regression to avoid") rather than inventing a new bar this rewrite then fails to clear.
- **Bundle metadata was filled in for real** (`category`, `shortDescription`/`longDescription`, `copyright`, `homepage`, Linux `deb.recommends: [ffmpeg]` — a soft dependency, not `depends`, since the BYO philosophy above means the app still works pointed at a manually-supplied binary even without the system package). Along the way, the Tauri CLI itself flagged a real, pre-existing issue: the bundle identifier `com.orangeloaf.app` ends in `.app`, which conflicts with the macOS bundle-extension convention. Fixed to `dev.orangeloaf.desktop` (matching the `dev`/`orangeloaf` qualifier/organization pair `directories::ProjectDirs` already uses throughout the Rust backend) — done now, before any real distribution ships with the old one, since changing a bundle identifier after the fact is far costlier (macOS treats it as a different app for preferences/updates/Gatekeeper history).
- **Two GitHub Actions workflows were added**: `ci.yml` (typecheck/lint/format/test/build on every push and PR — not explicitly asked for by this milestone's bullets, but a packaging milestone with zero existing CI felt like the wrong moment to skip it, and it's cheap) and `release.yml` (the actual ask). **Neither has been pushed or run** — writing and locally validating the workflow YAML is local, reversible work; actually pushing a tag/triggering a release workflow run is a separate, visible, shared-state action this session didn't take without asking first (see the conversation for that check-in).
- **Only the macOS `.app` bundle was actually built and smoke-tested locally** (`bun run tauri build --bundles app`, before the Linux-only pivot below) — this is a single macOS dev machine with no cross-compilation toolchain for Linux or Windows. The macOS smoke test itself hit a real environment limit worth recording accurately rather than glossing over: this execution environment has no attached display (`screencapture` fails with "could not create image from display"), so the packaged app's window could never be visually confirmed on-screen here, the same reason `.dmg` creation fails (its Finder-styling step needs a real AppleScript-addressable Finder window, which times out with `AppleEvent timed out (-1712)` in a display-less session — the `.app` bundle itself builds and launches fine; only the cosmetic disk-image-styling step is affected). What *was* verified: the built binary is a real signed-ad-hoc arm64 Mach-O, `Info.plist` correctly reflects the configured metadata, and running it directly produces no crash and no stderr output over a real launch window.

**Linux-only pivot (2026-07-29, user decision, supersedes the above 3-platform scope):** the user explicitly decided to drop Windows and macOS from the release pipeline entirely — both need real code-signing (and macOS additionally needs notarization) to avoid the OS itself flagging/blocking a fresh install, and neither is worth chasing for this project. `tauri.conf.json`'s `bundle.targets` is now `["deb", "rpm", "appimage"]` only (was `"all"`); the `windows.nsis` config block and the `icon.icns`/`icon.ico` entries were removed as no-longer-relevant. `release.yml` dropped its 3-platform matrix for a single `ubuntu-22.04` job. Anyone who wants a Windows or macOS build can still check out the repo and run `cargo tauri build`/`just build` themselves — the underlying Rust/Tauri code stays fully cross-platform (nothing OS-specific was added to the *application* to make this decision, only to the *release pipeline*); they just won't get a signed, notarized, one-click installer from this project's own CI. **This pivot is itself unverified on real Linux** for the same reason the original 3-platform scope was only ever partially verified: this is a single macOS dev machine, with no Linux environment (and no Docker) available to actually run `cargo tauri build`/produce a real `.deb`/`.rpm`/`.AppImage` here. The `tauri.conf.json` changes and `release.yml` are believed correct against Tauri's own schema (checked directly against the installed `@tauri-apps/cli`'s `config.schema.json` for this exact version, not guessed from memory) but the *first* real confirmation that a Linux build actually produces working artifacts will be either the user's own machine or a real GitHub Actions run.

### M15 — Polish & full parity pass
- Walk **Appendix B** literally, screen by screen, confirming every legacy control has a home (or a deliberate, documented cut — e.g. the already-dead `LargePreview`/`HDR10PlusInjectWindow`/`AudioSelect`-stub features are candidates to drop rather than resurrect, per the research's own recommendation).
- Visual QA against the two approved mockups + the extended design system for the 3 screens not yet mocked (History/Profiles/Settings) — extend `design/design-system.md`'s tokens to those screens without inventing new colors/fonts (same fidelity constraint used to build the first two mockups).
- Accessibility pass (WCAG AA contrast — the chosen dark/near-white/indigo palette clears this comfortably by construction, verify anyway).

**Implementation notes (this milestone is itself an audit — its output is mostly findings, not new features):**

**Accessibility pass — a real computed audit, not an assumption.** Computed exact WCAG relative-luminance contrast ratios for every real foreground/background pairing actually used in the app (not just spot-checked): base-content on all three surface steps is 17–18:1 (no concern), the `text-base-content/N` opacity ladder used throughout for secondary/muted text is AA-safe down to 40% (3.52–3.60:1, large-text/UI-component threshold) and only actually fails below that at 30%/lower, which the app *does* use — but exclusively on purely-decorative empty-state icons that are always paired with an adjacent text message (`QueueView`/`HistoryView`/`ProfilesView`'s empty states, `HistoryEntryRow`'s no-thumbnail fallback), which WCAG 1.4.11 explicitly exempts. **One real, confirmed violation was found and fixed**: `--color-primary-content` (the text/icon color painted on `btn-primary`/`bg-primary` fills — the app's single most important button, Convert's own primary CTA, among others) was `#f4f5f8` against `--color-primary: #5e6ad2`, measuring 4.31:1. This isn't a borderline UI-component case — DaisyUI's `btn-sm`/`btn-xs` (used for every `btn-primary` in this app) render at 12px/11px, font-weight 600, well under WCAG's large-text threshold (~18.66px bold), so the strict 4.5:1 text ratio applies and this genuinely failed it. Fixed by changing `primary-content` to pure `#ffffff` (4.70:1) — a minimal, semantically-scoped fix (this token only ever paints text *on* the primary fill, so nothing else is affected) rather than touching the primary accent color itself, which is a real brand/design decision already approved via the mockups, not this audit's to relitigate. One latent (not fixed) finding for awareness: `error-content` on `error` measures 3.19:1 (real AA-normal-text fail), but no `badge-error`/`bg-error`-with-text usage exists anywhere in the app today to actually expose it — every real error indicator uses `text-error` directly on a dark surface instead, which measures 5.73:1 (safe). Worth re-checking if a filled error badge/button is ever added.

**Visual QA**: re-screenshotted History/Profiles/Settings fresh (beyond each one's own build-time verification) specifically to check cross-screen consistency against `design/design-system.md`'s tokens. One real, intentional simplification confirmed (not a bug): the design doc specifies four background elevation steps (app/rail/**main-content**/raised-surface: `#08090a`/`#0c0d0f`/`#0b0c0e`/`#111318`), but the implemented theme has three (`base-100`/`base-200`/`base-300`), with the app shell's main content area using `base-100` rather than a fourth near-identical shade — the difference between `#08090a` and `#0b0c0e` is a handful of RGB units, imperceptible in practice, and inventing a barely-distinguishable fourth token would add theme surface area for no visible benefit. Similarly, the doc's separate discrete "secondary text" (`#b9bdc6`) / "muted text" (`#7d828c`) hex values were implemented as opacity steps on `base-content` instead (`/70`, `/50`, etc.) — same visual hierarchy outcome, one token family instead of three, and it's what the contrast audit above actually measured. Everything else (hairline treatment, card/surface radius, spacing density, mono-for-technical-content) matches consistently across all 5 views.

**Appendix B literal walkthrough** — status of every row, so nothing is silently missing:

*Already fully covered by an existing milestone's own scope note (not re-litigated here, just indexed)*: HDR10+/Dolby Vision (M8), rigaya/NVENC/QSV/VCE/VAAPI encoder families and GPU capability probing (M5/M12), subtitle burn-in + disposition popup + external subtitle files + drag-to-reorder tracks + PGS→SRT OCR (M7), Data/Cover panels (M7), the full ~28-control x265 Quality inventory beyond preset/tune/profile/CRF — bitrate mode, passes, adaptive quantization, frame threads, max-muxing-queue, the HDR10/repeat-headers/intra-refresh/intra-smoothing/lossless checkboxes, max-B-frames, B-adapt, free-form params, HDR10+ metadata picker, custom-ffmpeg field (M3/M4 — every encoder ships a deliberately-curated *subset* per its own doc comment, not the full legacy inventory), profile rename/duplicate/delete-scope resolution + Title/Track-index audio-match types + subtitle auto-selection rules (M10), history max-items selector + a separate Details view (M11), GPU detection + tool auto-download (M12), full-app string coverage beyond Settings+shell (M13), ffmpeg bundling + codesigning (M14).

*Newly confirmed here, not previously written down anywhere*:
- **No profile quick-picker on the Convert header.** Applying a profile is Profiles-view-only (`Apply` navigates to Convert after setting `encode.settings`/`filters`/`audioTracks`) — there's no dropdown directly in Convert's own header the way the legacy main window has. Not fixed now: a real, if small, IA decision (where does a second profile-selection entry point live, and does it risk fighting the one already built) worth its own consideration rather than bolting on during an audit pass.
- **No chroma-subsampling badge** among Convert's video-info badges (codec/resolution/bit-depth/HDR10 are shown; 4:2:0 vs 4:2:2 vs 4:4:4 is not, though `pix_fmt` — which encodes this — is already probed and available).
- **No resolution-preset dropdown or "long-edge" scale mode** — Convert's Scale section is plain free-form width/height number inputs only; the legacy's ~20 common-resolution presets and auto-long-edge mode don't exist.
- **No crop auto-detect** (ffmpeg's `cropdetect` filter, run against a real frame range to suggest black-bar crop values) — crop is manual-only (visual drag tool or typed T/L/R/B).
- **No "set trim from current preview position"** button — trim start/end are typed/stepped number inputs only; the scrubber position and trim fields aren't cross-wired.
- **No Remove Metadata / Copy Chapters / Remove HDR filter toggles** — `FilterSpec` has deinterlace/crop/scale/rotate/flip only; these three legacy options were never added to the model at all.
- **No "Advanced" accordion section for any encoder** (frame-rate override+vsync, speed, tone-map, brightness/contrast/saturation, denoise, deblock, color primaries/transfer/space override, VBV maxrate/bufsize, video/track title) — this is the same gap M10's own note already flagged from the *Profile* side ("`AdvancedOptions` isn't part of the `Profile` model because none of those controls exist anywhere in this app's pipeline yet, for any encoder"); recorded here as the primary-side version of the same missing feature, not a second independent one.
- **No Source Details view** (a per-stream raw ffprobe metadata dump) — `VideoInfo` already carries a meaningful subset (not the full raw JSON), but there's no screen that displays it verbatim.
- **No Debug view** (config/queue/encoders YAML dumps) — the checklist itself marks this low-priority; left undone.
- **Concatenation Builder and Multiple-Files batch queue dialogs don't exist at all**, and — distinct from every other gap in this list — were never actually scoped into *any* milestone's bullet list in §6 to begin with, only ever named in Appendix B's checklist. This is a genuine planning gap this audit surfaces rather than a deferral anyone actually decided on: both are real, self-contained features (folder scan → probe → reorderable table → commit, the same underlying table pattern for both) that would need their own milestone-sized slice of design + implementation + real-file verification, not something to improvise correctly during a polish pass.

None of the above are fixed in this milestone — M15's own scope is the audit itself (confirm every control has a home or a documented reason it doesn't), and the volume of genuinely new functionality above (two full dialogs, an Advanced settings tab, several Convert-screen controls) is multiple milestones' worth of real work, not polish. Recorded here so it's a decision for whoever picks up next, not a silent gap.

**Post-M15 follow-up (2026-07-29): NVENC detection + persistent status bar.** Two specific gaps closed after the audit above, both user-directed rather than self-initiated:
- **Real NVENC detection** (`src-tauri/src/tools.rs::detect_nvenc`) — partially answers line 566's "GPU detection ... (M12)" deferral, though only for NVIDIA/NVENC specifically; QSV/VAAPI/AMF detection and tool auto-download are still undone. Checked the same way `detect_tool` checks ffmpeg/ffprobe themselves: by actually invoking `ffmpeg -c:v h264_nvenc` against a 1-frame synthetic input, not by parsing `-encoders` output (which only proves ffmpeg was *compiled* with NVENC support, not that a real GPU+driver can use it right now). This is ffmpeg's own built-in encoder, distinct from and much simpler than the Windows-only rigaya `NVEncC.exe` family M5/M12 already deferred — plain `-c:v h264_nvenc` works cross-platform given a real NVIDIA GPU, which is why it's newly in scope now that Linux (where NVIDIA GPUs are the realistic hardware-encoding target for this project) is the actual release platform. Verified empirically on the only side observable on this dev machine: correctly reports `false` on this no-NVIDIA-GPU Mac (real test, `#[ignore = "requires system ffmpeg"]`); the positive case is unverified here for the same reason it's always been unverified — no NVENC-capable machine in this environment.
- **Persistent footer status bar** (`src/lib/components/StatusBar.svelte`, mounted in `App.svelte` below every view) — closes the drift between the built app and `design/mockups/*.html`'s footer (FFmpeg version / NVENC availability / queue count / app version), found by re-comparing the built UI against the already-tracked mockups rather than any newly-discovered "nicer" design source (`.superdesign/design-system.md` was checked and confirmed to be only a stale pre-rename duplicate of `design/design-system.md`, not a separate design). Styled against this app's own token system (`bg-base-200`/`border-hairline`/`text-base-content/50`, matching `IconRail`'s existing chrome treatment) rather than copying the mockup's raw Tailwind-CDN literals verbatim; the mockup's `text-accent` on the build-version text was deliberately not carried over, since `design/design-system.md`'s own rationing rule confines the accent color to 4 specific uses the footer isn't one of. A matching live queue-count badge was added to `IconRail`'s Queue nav icon (`bg-warning`/`text-warning-content`, matching the mockup's badge color exactly — it turned out to already be this theme's `--color-warning`). Both the FFmpeg-version and queue-count text stay hardcoded English, matching every other not-yet-i18n'd view/string per M13's own documented scope (Settings + `IconRail` only) — no new translation pattern was invented for a couple of dynamic-count strings.
- Still open, reported but not built (only the three items above were actually asked for): the profile quick-picker, CRF/Bitrate + two-pass toggles, resolution presets, drag-to-reorder, Load/Save Queue, aggregate ETA, log filtering, and completed-row file-size/duration gaps already listed above remain exactly as the M15 audit found them.

---

## 7. Testing strategy

- **Rust unit tests**: one test module per encoder verifying exact argv output for known input settings (mirrors `tests/encoders/test_*_command_builder.py` in the legacy repo — port the same fixture philosophy: given these settings, assert this exact argv). Filter-chain ordering (especially the subtitle-burn-in overlay-first/filters-first branch) gets its own dedicated matrix test. HDR formatters get golden-value tests, especially the SVT-AV1 rescale.
- **Rust integration tests**: spawn real ffmpeg against small fixture media (the legacy `tests/media`/`tests/assets` directories are a good seed set) for at least one encoder per family (ffmpeg-native, rigaya-if-available, gifski pipe).
- **Frontend**: component tests for the field-schema renderer (§5.2) and the crop-tool coordinate transforms (pure functions, easy to test in isolation); store tests for the derived command-preview and debounced-thumbnail logic.
- **E2E**: Playwright or WebdriverIO+`tauri-driver` over the built app for the M3 "golden path" (load → configure → convert → verify output exists) and the M9 queue-recovery scenario (kill mid-queue, relaunch, confirm prompt).

## 8. Open product decisions

These are explicitly **not** implementation details — they change scope and should be confirmed before or during the relevant milestone, not defaulted silently:

1. **FFmpeg distribution** (§3.2): BYO (parity, no new licensing exposure) vs. bundled sidecar (better UX, reopens the GPL-bundling question the legacy app deliberately avoided).
2. **Profiles editability** (§4.10/M10): keep legacy's create-only + silent-revert-of-default-profile-edits behavior, or fix it (rename/duplicate/edit) as part of the rewrite.
3. **Multi-instance support** (§4.9): legacy allows concurrent instances and defends the queue file with an advisory lock; decide whether the Rust app enforces single-instance instead (simpler) or needs to preserve multi-instance semantics.
4. **Cross-platform GPU/tool auto-download** (M12): legacy only auto-detects/downloads on Windows. Invest in Linux/macOS parity, or explicitly scope it out like the source app does.
5. **Config/profile/queue/history format & migration**: keep YAML (human-readable, matches legacy files byte-for-byte-ish, enables a "import my old FastFlix settings" feature) vs. JSON (more natural in the Rust/serde/TS world). If YAML, a legacy-config importer becomes cheap; if JSON, decide whether an importer is worth building anyway.
6. **Codesigning/notarization** (M14): the legacy app has never shipped signed/notarized builds (CI steps exist but are commented out) — decide if this rewrite is the moment to actually clear that bar.
7. **History "apply" scope** (M11): keep legacy's deliberate "encoder settings only" limitation, or expand to a full job clone (source/trim/tracks included) now that the schema can support it.
8. **Light theme**: the approved design direction (§2) is dark-only, matching the style anchor's `Theme Mode: Dark Mode (enforced)`. Confirm whether a light theme is actually required for parity, or whether dark-only is an acceptable (likely improved) scope reduction.

---

## 9. Appendices

### Appendix A — Encoder reference (condensed; full detail in the research transcript this plan was built from)

**Shared ffmpeg-native argv assembly order** (`generate_ffmpeg_start` → filters → `generate_ending`): `ffmpeg` → hwaccel/opencl init → `-y` → fast-seek `-ss/-to` (before `-i`) → source-fps override → concat demuxer flags → `-i <source>` → extra subtitle inputs (each with their own seek if fast-seek) → slow-seek `-ss/-to` (after inputs) → title metadata → muxing-queue-size → `-map 0:<track>` (only if no filter graph) → `-fps_mode` → filter-complex fragment (if any) → `-c:v <codec>` → `-pix_fmt` → maxrate/bufsize → track-title metadata; then rotation-tag-clear (if applicable) → `-map_metadata` → `-map_chapters` → output-fps → audio fragment → subtitle fragment → cover/attachment fragment → data/attachment stream maps → output path (or null device).

**Filter chain, fixed order**: deinterlace → crop → scale+setsar (skipped for VAAPI) → rotate → vflip → hflip → speed → deblock → denoise → eq → [VAAPI hwdownload/format/hwupload wrap] → HDR tonemap (opencl/vaapi/software-zscale variants) → custom filters (GIF's `fps,palettegen` always lands here, last) → subtitle burn-in wrap (**picture-based: overlay first, then the rest; text-based: rest first, then `subtitles=` filter**).

**Per-encoder summary**:

| Encoder | Binary | Codec flag | 2-pass | HDR10 | HDR10+ | DV | Notable quirk |
|---|---|---|---|---|---|---|---|
| HEVC (x265) | ffmpeg | `libx265` | yes | `-x265-params master-display=/max-cll=/hdr10=` | `dhdr10-info='path'` | no | validity-filtered color enums; relative pass-log path |
| AVC (x264) | ffmpeg | `libx264` | yes | n/a | n/a | n/a | cleanest baseline template; 8-bit only |
| AV1 (AOM) | ffmpeg | `libaom-av1` | yes (bitrate) | **none emitted** (log line only) | log-only | no | verify empirically before assuming parity |
| AV1 (SVT-AV1) | ffmpeg | `libsvtav1` | yes | `-svtav1-params mastering-display=.../enable-hdr=1` (rescaled) | not supported | no | field `speed` → flag `-preset` mismatch |
| AVIF (SVT-AV1) | ffmpeg | `libsvtav1 -f avif` | never | same style | no | no | `single_pass` forced True; qp_mode defaults `qp` not `crf` |
| AV1 (rav1e) | ffmpeg | `librav1e` | yes | `-rav1e-params mastering_display=/content_light=` | no | no | qp is 0–255 scale, not 0–63 |
| VP9 | ffmpeg | `libvpx-vp9` | yes (opt-out) | commented out | no | no | `realtime` quality forces single-pass |
| VVC | ffmpeg | `libvvenc` | yes | `-vvenc-params MasteringDisplayColourVolume=/MaxContentLightLevel=` | not supported | no | x265-clone template — the model for slotting in a new codec |
| WebP | ffmpeg | `libwebp` | no | n/a | n/a | n/a | audio/subs disabled; effective `pix_fmt` forced `yuv420p` at the UI layer, not the model default |
| GIF | ffmpeg (2 cmds) | palettegen→paletteuse | n/a | n/a | n/a | n/a | scale/crop must be identical in both commands or colors mismatch |
| GIF (gifski) | ffmpeg\|gifski (real pipe) | yuv4mpegpipe→gifski | n/a | n/a | n/a | n/a | only shell/string command in the legacy app — port as a real OS pipe, not `shell=true` |
| Copy | ffmpeg | `copy` | no | n/a | n/a | n/a | display-matrix rotate/flip metadata only, MP4/MOV/MKV/M4V only |
| Modify | ffmpeg | `copy` | no | n/a | n/a | n/a | bypasses shared builder; adds external audio/subtitle track |
| HEVC/AV1 (NVENC, ffmpeg) | ffmpeg | `hevc_nvenc`/`av1_nvenc` | yes (dual: `-pass`+`-2pass 1`) | none (ffmpeg encoder has no equivalent) | no | no | `-qp:v` for CQ mode, not `-crf` |
| H264/HEVC (VideoToolbox) | ffmpeg | `h264_videotoolbox`/`hevc_videotoolbox` | yes | color-details only | no | no | byte-identical twins except codec string |
| VAAPI ×4 (H264/HEVC/VP9/MPEG2) | ffmpeg | `*_vaapi` | **no** (commented out) | none | no | no | needs explicit `-init_hw_device`+hwdownload/hwupload wrap |
| NVEncC / QSVEncC / VCEEncC × HEVC/AVC/AV1 (9 total) | external exe | native `-c <codec>` | native `--multipass` | `--master-display`/`--max-cll` | `--dhdr10-info copy` | `--dolby-vision-rpu copy` (+`-profile copy`, +`-prm crop=true` if cropped) | ffmpeg plays **no role**; own VPP flags for crop/rotate/flip/scale; **`--crop` order is L,T,R,B**, unlike ffmpeg's filter |

Rigaya-family shared flags (device/reader/trim/seek/fps/rotate/flip/resolution/crop/metadata/chapters/codec/rate-control/VBV/bframes/preset/tier/level/lookahead/AQ/color/HDR/bit-depth/sync/interlace/deinterlace/tonemap/split/metrics/output) plus AMD-only (`--vbaq/--pe/--pa .../--motion-est`) and Intel-only (`--adapt-ref/-ltr/-cqm`) extras: see the full flag table in the research transcript — reproduce verbatim, this is the part of the app with the least room for creative reinterpretation.

**Audio quality→bitrate table** (index 0–9, `base = [120,96,72,48,24,24,16,8,8,8]` kbps/channel):

| Codec | Formula |
|---|---|
| `libopus` | `-vbr on -b {base*channels}k` |
| `aac` | `-q {[2,1.8,1.6,1.4,1.2,1,0.8,0.6,0.4,0.2][i]}` |
| `libfdk_aac` | `-q {[1,1,2,2,3,3,4,4,5,5][i]}` |
| `libvorbis` | `-q {[10,9,8,7,6,5,4,3,2,1][i]}` |
| `libmp3lame` | `-q {i}` |
| `ac3`/`eac3`/`truehd` | `-b {base*channels*4}k` |
| other | `-b {base*channels}k` |
| `flac,truehd,alac,tta,wavpack,mlp` | lossless — no bitrate/quality flag at all |

**Naming template variables** (pre-encode): `{source} {datetime} {date} {time} {rand_4} {rand_8} {encoder} {codec} {preset} {crf} {bitrate} {pix_fmt} {resolution} {source_resolution} {source_duration} {source_fps} {source_codec} {source_bitrate} {audio_codec} {audio_channels} {color_space} {color_primaries} {hdr}`. Post-encode (resolved via on-disk placeholder-token replacement after encoding finishes): file size, achieved video/audio/total bitrate, elapsed time.

### Appendix B — Full screen/control parity checklist

Use this as the literal "is the rewrite done" checklist — every row needs a home in the new IA (accordion section, dedicated view, or a documented, deliberate cut):

**Main shell**: menu-bar equivalents (File/Profiles/Tools/History/Help actions) → Settings view + command palette or menu; Source/Encoder/Profile pickers + Add-to-Queue/Convert → Convert header (built, see mockup); video-info badges (codec/bit-depth/chroma/HDR10/HDR10+) → built, see mockup.

**First tab strip** → folded into Convert's Filters/Size accordion section: Resolution (Auto/Long-Edge/Width/Height/Custom + ~20 presets), Rotate, Flip; Start/End time (fast/exact seek, ± steppers, "set from preview"); Crop (Auto/black-bar-detect, Reset, Visual Crop, T/L/R/B fields); Options (Remove Metadata, Copy Chapters, Deinterlace, Remove HDR).

**Second tab strip** → Convert's main accordion + dedicated views:
- **Quality** → per-encoder accordion section, field-schema-rendered (§5.2). x265 alone: preset/tune/profile/bit-depth, CRF-or-bitrate mode box (+passes), adaptive quantization, frame threads, max-muxing-queue, force-HDR10/HDR10-opt/repeat-headers/intra-refresh/intra-smoothing/lossless checkboxes, max-B-frames, B-adapt, free-form params field, HDR10+ metadata file picker + extract button, custom-ffmpeg field, guide links.
- **Audio** → per-track: title, enabled, reorder, duplicate (hidden for rigaya), delete, language, dispositions popup, conversion popup (codec/quality-or-bitrate/downmix).
- **Subtitles** → per-track: preserve, dispositions, language, burn-in (mutually exclusive across tracks, forces exact-seek), extract (plain, or PGS's fast-.sup-vs-OCR-.srt submenu), external-file add/remove.
- **Data** (distinct from Cover — both required) → preserve toggle per attachment/font/data stream, container-compatibility graying.
- **Cover** → 4 checkboxes (copy cover/small-cover/landscape/small-landscape) + 2 image previews + 2 manual-file pickers.
- **Advanced** → frame-rate override+vsync, video speed, tone-map, brightness/contrast/saturation, denoise (method×strength), deblock (strength×size), color primaries/transfer/space, VBV maxrate/bufsize, video/track title.
- **Source Details** → per-stream raw metadata dump.
- **Raw Commands** → one block per actual Command, copy/save-as-script actions.
- **Encoding Status** → live log + time-left/elapsed/size-estimate → **relocated**, not dropped: lives behind each Queue row's disclosure chevron (§2.4), not as a Convert-screen tab.
- **Encoding Queue** → see Queue view (mockup built) + **new** aggregate progress.
- **Debug** (dev-mode only) → config/ffmpeg-details/queue/encoders/audio-encoders/current-video YAML dumps; low priority.

**Profiles**: new/current-settings/delete menu actions; profile window's Primary/Video/Audio(pattern-match rules)/Subtitles/Data/Advanced tabs.

**Settings**: General (paths would be a mistake here — paths live in Application Locations tab specifically; General has work dir, language, theme, logging level, crop-detect points, UI scale, ~12 behavior checkboxes, OpenCL status+redetect, default output/source dirs), Output Naming (template field + variable chips + live preview + reference table), Audio Encoders (curated allowlist), Application Locations (ffmpeg/ffprobe/nvencc/vceencc/qsvencc/hdr10+parser/gifski paths + "Detected External Programs" status list).

**History window**: max-items selector, clear-all, per-entry thumbnail/summary/Apply/Details/Delete.

**Other dialogs**: Concatenation Builder (folder scan→probe→reorder table→write concat-demuxer file); Disposition popup (default/forced + one mutually-exclusive extra flag); Audio Conversion popup (codec/quality-or-bitrate/downmix); Multiple-Files batch queue (same table pattern as Concat + a profile picker applied to all).

**Deliberately not required for parity** (dead/unreachable in the legacy app itself — confirm-and-cut rather than resurrect, unless a product reason emerges): Large Preview popout, HDR10+ Inject window, the empty `AudioSelect` stub.

### Appendix C — Reference material locations

- Cloned source (read-only reference, not shipped): `temp/FastFlix/`
- Design tokens: `design/design-system.md`
- Verified mockups: `design/mockups/convert-screen.{html,png}`, `design/mockups/queue-screen.{html,png}`
- Legacy screenshot for before/after comparison: `temp/FastFlix/docs/gui_preview.png`
- Legacy's own architecture notes (useful cross-check while porting): `temp/FastFlix/CLAUDE.md`
