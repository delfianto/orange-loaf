# OrangeLoaf

A desktop video transcoding app — Tauri (Rust) + Svelte 5 + DaisyUI 5, wrapping FFmpeg and
~30 software/hardware encoder backends.

OrangeLoaf's design and encoder-integration approach were informed by studying
[FastFlix](https://github.com/cdgriffith/FastFlix) (MIT) as prior art — understanding how a
mature, 30-encoder FFmpeg GUI has to work. No FastFlix source, assets, or branding are used;
OrangeLoaf is an original implementation with its own design system. See `notes/plan.md` for the
full architecture writeup and clean-room notice.

**Linux only.** Packaging/CI targets Linux (.deb/.rpm/.AppImage) exclusively — no signed/notarized
Windows or macOS builds are produced by this project. The application code itself is still plain
cross-platform Rust/Tauri; if you want to run this on Windows or macOS, clone the repo and build
it yourself (`cargo tauri build`) — see `notes/plan.md` M14 for why this project's own CI doesn't
do that for you.

## Stack

- **Backend**: Rust (edition 2024), Tauri 2
- **Frontend**: Svelte 5 (runes), TypeScript 7, Tailwind CSS 4, DaisyUI 5
- **Tooling**: Bun, Vite, ESLint 9 (flat config, type-aware), Prettier, Clippy (`pedantic`), [`just`](https://github.com/casey/just)

## Prerequisites

- [Bun](https://bun.sh) 1.3+
- Rust 1.85+ (edition 2024)
- [`just`](https://github.com/casey/just) (optional but recommended — see below)
- FFmpeg + FFprobe on `PATH` (or configured in Settings — see `notes/plan.md`)
- Linux only for the packaged build: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`,
  `librsvg2-dev`, `patchelf` (matches `.github/workflows/ci.yml`'s setup)

## Development

If you have [`just`](https://github.com/casey/just) installed, it's the easiest way to work in
this repo — run `just` with no arguments to list every available recipe:

```bash
just install   # bun install
just dev       # launch the app (Rust + Vite dev server, hot-reloading)
just check     # everything CI runs: typecheck, lint, format-check, test, fmt-check, clippy, cargo test
just build     # full release build — Linux installers (.deb/.rpm/.AppImage)
```

Without `just`, the equivalent bun/cargo commands:

```bash
bun install
bun run tauri dev     # launch the app (Rust + Vite dev server)

bun run typecheck     # tsc -b --noEmit (TypeScript 7 native compiler)
bun run lint          # eslint . (type-aware, strict)
bun run format        # prettier --write .
bun run test          # vitest run
bun run build         # production frontend build only

cd src-tauri
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo test                # excludes real-ffmpeg-gated tests
cargo test -- --ignored   # the real-ffmpeg-gated tests (needs ffmpeg/ffprobe on PATH)
```

## Project layout

```
src/               Svelte frontend (views, components, runes-based stores)
src-tauri/         Rust backend (Tauri commands, encoder logic, config/queue/profiles)
design/            Design system tokens + verified DaisyUI mockups
notes/plan.md      Full implementation plan, architecture research, and build milestones
messages/          i18n message catalogs (en/es/fr/de/ja) — see notes/plan.md M13
project.inlang/    paraglide-js i18n project config
justfile           Local dev recipes (just --list)
temp/              Local-only reference clone of FastFlix (gitignored, not part of this project)
```

## Status

All milestones (M0–M15) in `notes/plan.md` §6 are implemented — see that file's per-milestone
"Implementation notes" for what shipped, what was deliberately deferred, and why. See recent
commits for the latest changes.
