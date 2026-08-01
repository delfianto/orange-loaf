# 🍊🍞 OrangeLoaf

_A desktop video transcoder that treats FFmpeg like the feral genius it is —
brilliant, 400 flags deep, and one typo away from producing a 4 KB file that is
somehow also nine hours long._

OrangeLoaf wraps FFmpeg and ~30 software/hardware encoder backends (x264, x265,
SVT-AV1, NVENC, QSV, VAAPI, VideoToolbox, and other alphabet soup) in a calm,
dark, keyboard-friendly desktop app — so you can compress your Blu-ray rips
without hand-typing `-map 0:a:1 -c:v libx265 -x265-params …` at 2 a.m. like a
wizard who has come to resent his own spellbook.

Built with Tauri (Rust) + Svelte 5 + DaisyUI 5. It is a loaf. It is orange. Do
not eat it.

## What it actually is (the honest paragraph)

A GUI for batch-transcoding video: pick a file, tune codec / quality / HDR /
audio / subtitle settings, queue up a pile of jobs, hit go, and walk away for
several hours the way FFmpeg always intended. The entire reason it exists is to
fix the classic failure mode of tools in this space — a ten-tab wall of tiny
unlabeled checkboxes with the visual hierarchy of a tax form — with something
that tells you what matters _right now_.

The old way: "which of these sixty controls keeps the Japanese audio and drops
the director's commentary?" The OrangeLoaf way: you set `jpn` once in Settings
and it just does that, forever, on every file. Groundbreaking. We're as
surprised as you are.

## The lawyer paragraph (sincerity mode: engaged)

OrangeLoaf's design and encoder-integration approach were informed by studying
[FastFlix](https://github.com/cdgriffith/FastFlix) (MIT) as prior art — i.e. we
stood on the shoulders of a giant to learn how a mature, 30-encoder FFmpeg GUI
has to work, then climbed down and wrote our own. **No FastFlix source, assets,
or branding are used.** OrangeLoaf is an original implementation with its own
design system. The full clean-room notice and architecture writeup live in
`notes/plan.md`.

## 🐧 Linux only (a lifestyle choice)

Packaging and CI produce **Linux** installers only — `.deb`, `.rpm`,
`.AppImage`. No signed or notarized Windows/macOS builds come out of this
project, because herding code-signing certificates across three operating
systems is a hobby for people with more serotonin than we currently have.

The code itself is plain cross-platform Rust/Tauri, so if you're on Windows or
macOS and feeling brave, clone it and run `cargo tauri build` yourself. It'll
probably work. See `notes/plan.md` M14 for the full "why our CI politely
declines to do this for you" saga.

## Stack (buzzword bingo)

- **Backend**: Rust (edition 2024), Tauri 2 — memory-safe, so the only thing
  that panics is you, when an encode dies at 98%.
- **Frontend**: Svelte 5 (runes), TypeScript 7, Tailwind CSS 4, DaisyUI 5.
- **Tooling**: Bun, Vite, ESLint 9 (flat config, type-aware, judgmental),
  Prettier, Clippy on `pedantic` (it has opinions and shares them freely), and
  [`just`](https://github.com/casey/just).

## Prerequisites (things that must exist before joy)

- [Bun](https://bun.sh) 1.3+
- Rust 1.97+ (edition 2024)
- [`just`](https://github.com/casey/just) — optional, recommended, spiritually correct
- **FFmpeg + FFprobe** on your `PATH` (or point at them in Settings). This is
  the entire engine. Without it, OrangeLoaf is a very handsome window that
  silently judges your video files.
- Linux packaged build only: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`,
  `librsvg2-dev`, `patchelf` (matches `.github/workflows/ci.yml`).

## Development (`just` do it)

With [`just`](https://github.com/casey/just) installed — run `just` on its own
to list every recipe:

```bash
just install   # bun install
just dev       # launch the app (Rust + Vite dev server, hot-reloading)
just check     # everything CI runs: typecheck, lint, format-check, test, fmt-check, clippy, cargo test
just build     # full release build — Linux installers (.deb/.rpm/.AppImage)
```

Prefer the artisanal, hand-typed experience? The raw bun/cargo equivalents:

```bash
bun install
bun run tauri dev     # launch the app (Rust + Vite dev server)

bun run typecheck     # tsc -b --noEmit (TypeScript 7 native compiler)
bun run lint          # eslint . (type-aware, strict)
bun run format        # prettier --write .
bun run test          # vitest run
bun run build         # production frontend build only

cd src-tauri
cargo clippy --all-targets -- -D warnings   # no warnings left behind
cargo fmt --check
cargo test                # excludes the real-ffmpeg-gated tests
cargo test -- --ignored   # the real-ffmpeg tests (needs ffmpeg/ffprobe on PATH)
```

## Project layout (where the bodies are buried)

```
src/               Svelte frontend (views, components, runes-based stores)
src-tauri/         Rust backend (Tauri commands, encoder logic, config/queue/profiles)
design/            Design system tokens + the DaisyUI mockups we swear we followed
notes/plan.md      The full plan, architecture research, and build milestones (it's long; bring snacks)
messages/          i18n catalogs (en/es/fr/de/ja) — complain about codecs in five languages
project.inlang/    paraglide-js i18n config
justfile           Local dev recipes (just --list)
temp/              Local-only reference clone of FastFlix (gitignored, not part of this project)
```

## Status

All milestones (M0–M15) in `notes/plan.md` §6 are implemented — see that file's
per-milestone notes for what shipped, what was deliberately deferred, and the
excuse attached to each. Recent commits carry the UI glow-up (Convert and Queue
screens rebuilt to their mockups, a two-column Settings page, per-language track
defaults for archivists) and one heroic serde bugfix that taught everyone
present the difference between `snake_case` and `camelCase` the hard way.

Now go transcode something. The loaf is warm.
