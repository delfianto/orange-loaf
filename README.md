# Voltaic

A desktop video transcoding app — Tauri (Rust) + Svelte 5 + DaisyUI 5, wrapping FFmpeg and
~30 software/hardware encoder backends.

Voltaic's design and encoder-integration approach were informed by studying
[FastFlix](https://github.com/cdgriffith/FastFlix) (MIT) as prior art — understanding how a
mature, 30-encoder FFmpeg GUI has to work. No FastFlix source, assets, or branding are used;
Voltaic is an original implementation with its own design system. See `notes/plan.md` for the
full architecture writeup and clean-room notice.

## Stack

- **Backend**: Rust (edition 2024), Tauri 2
- **Frontend**: Svelte 5 (runes), TypeScript 7, Tailwind CSS 4, DaisyUI 5
- **Tooling**: Bun, Vite, ESLint 9 (flat config, type-aware), Prettier, Clippy (`pedantic`)

## Prerequisites

- [Bun](https://bun.sh) 1.3+
- Rust 1.85+ (edition 2024)
- FFmpeg + FFprobe on `PATH` (or configured in Settings once that lands — see `notes/plan.md`)

## Development

```bash
bun install
bun run tauri dev     # launch the app (Rust + Vite dev server)
```

Other scripts:

```bash
bun run typecheck     # tsc -b --noEmit (TypeScript 7 native compiler)
bun run lint           # eslint . (type-aware, strict)
bun run format         # prettier --write .
bun run build          # production frontend build
```

Rust checks (from `src-tauri/`):

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Project layout

```
src/            Svelte frontend (views, components, runes-based stores)
src-tauri/      Rust backend (Tauri commands, encoder logic, config/queue/profiles)
design/         Design system tokens + verified DaisyUI mockups
notes/plan.md   Full implementation plan, architecture research, and build milestones
temp/           Local-only reference clone of FastFlix (gitignored, not part of this project)
```

## Status

Implementation is tracked milestone-by-milestone in `notes/plan.md` §6. See recent commits
for current progress.
