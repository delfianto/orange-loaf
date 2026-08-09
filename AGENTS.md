# AGENTS.md

Coding-harness instructions for AI agents working in this repo. Read this
before touching anything. The README has the friendly version; this file is
the dry, exact version.

## What this project is

OrangeLoaf (`dev.orangeloaf.desktop`) is a Linux-first desktop GUI for FFmpeg,
built with **Tauri 2 (Rust, edition 2024)** + **Svelte 5 (runes)** + **Tailwind
CSS 4 / DaisyUI 5**. It wraps FFmpeg and ~30 encoder backends (currently
x264 / x265 / SVT-AV1 / H264+HEVC VideoToolbox are implemented) in a calm,
dark, keyboard-friendly single-window app. FFmpeg/FFprobe are **never bundled** —
the user points at their own install in Settings. Packaging is **Linux-only**
(`.deb`/`.rpm`/`.AppImage`); the Rust code is cross-platform but CI declines to
package for Windows/macOS (see `notes/plan.md` M14).

Project was renamed Voltaic → OrangeLoaf. Any leftover `VOLTAIC_*` env var or
"Voltaic" string is a bug; the canonical names are `ORANGELOAF_CONFIG`,
`ORANGELOAF_QUEUE`, `ORANGELOAF_HISTORY`, `ORANGELOAF_PROFILES`.

## Tech stack (one-liner each)

- **Frontend:** Svelte 5 (runes mode — `$state`, `$derived`, `$effect`,
  `{#key}`), TypeScript 7 (`@typescript/native` aliased to `typescript@7`),
  Vite 8, Tailwind 4 (CSS-first config in `src/app.css`, **no**
  `tailwind.config.js`), DaisyUI 5 (custom `orangeloaf` theme in `src/app.css`),
  `lucide-svelte` icons, `@fontsource-variable/inter` + `jetbrains-mono`.
- **Backend:** Rust edition 2024, `rust-version = "1.97"`, Tauri 2
  (+`tauri-plugin-opener`, `tauri-plugin-dialog`), serde/serde_json, tokio,
  uuid v4, `directories`, `which`, `base64`.
- **i18n:** Paraglide-JS via `@inlang/paraglide-js`, `globalVariable` +
  `baseLocale` strategy. `baseLocale=en`, locales `en/es/fr/de/ja`. Messages
  live in `messages/{locale}.json`; generated code is in `src/paraglide/`
  (**gitignored**, regenerated on build and by the `pretypecheck`/`prelint`
  hooks).
- **Tests:** Frontend — Vitest 4, node env, `src/**/*.test.ts`. Backend —
  inline `#[cfg(test)] mod tests` per file; tests needing real ffmpeg/ffprobe
  are `#[ignore = "requires system ffmpeg"]` (or ffprobe / libsvtav1 /
  VideoToolbox variants) and run via `cargo test -- --ignored`.
- **Tooling:** Bun 1.3+, `just`, ESLint 9 flat config (type-aware), Prettier
  (with `prettier-plugin-svelte`), Clippy on `pedantic` with `-D warnings`.

## Commands you must run

Prefer the `just` recipes — they match CI exactly. Run them from the repo root.

| Task                                                | Command                                                      |
| --------------------------------------------------- | ------------------------------------------------------------ |
| Install deps                                        | `just install` (=`bun install`)                              |
| Launch the app (Rust + Vite, hot-reload, port 1420) | `just dev` (=`bun run tauri dev`)                            |
| **Everything CI runs, one shot**                    | `just check`                                                 |
| Frontend typecheck                                  | `just typecheck` (=`bun run typecheck`)                      |
| Frontend lint                                       | `just lint` / `just lint-fix`                                |
| Frontend format check / write                       | `just format-check` / `just format`                          |
| Frontend unit tests                                 | `just test` (=`vitest run`, no watch)                        |
| Rust fmt check / write                              | `just fmt-check` / `just fmt`                                |
| Rust clippy (CI gate)                               | `just clippy` (=`cargo clippy --all-targets -- -D warnings`) |
| Rust unit tests                                     | `just test-rust` (=`cargo test`, skips `#[ignore]`d)         |
| Rust integration tests (needs ffmpeg on PATH)       | `just test-rust-real` (=`cargo test -- --ignored`)           |
| Production frontend build only                      | `just build-frontend` (=`bun run build`)                     |
| Full release build (Linux installers)               | `just build` (=`bun run tauri build`)                        |
| Release binary only, no installer (use off-Linux)   | `just build-bin` (=`bun run tauri build --no-bundle`)        |
| Clean build artifacts                               | `just clean`                                                 |

### Critical command gotchas

1. **Never invoke bare `tsc` or `eslint`.** The `pretypecheck` and `prelint`
   hooks in `package.json` run `paraglide:compile` first to regenerate
   `src/paraglide/`. Without that step, type resolution breaks for every file
   importing `m`. Always go through `bun run typecheck` / `bun run lint` (or
   the `just` recipes, which call them).
2. **Rust commands run in `src-tauri/`.** The `just` recipes `cd` there for
   you. If you invoke cargo directly, set the working dir to `src-tauri/`.
3. **`cargo test` skips `#[ignore]`d tests by design.** CI runs plain
   `cargo test` (no `--ignored`); real-ffmpeg integration tests run only
   locally with ffmpeg/ffprobe on PATH via `cargo test -- --ignored`.
4. **Clippy is `-D warnings` in CI.** Any new lint warning fails the build.
5. **Off-Linux, use `just build-bin`** (`--no-bundle`) for a faster iteration
   that doesn't require the installer toolchain.

## CI gates (what `just check` must pass)

From `.github/workflows/ci.yml`:

- **Frontend job:** `bun run typecheck`, `lint`, `format:check`, `test`, `build`.
- **Rust job:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` (no `--ignored`).

If you change anything, run `just check` before considering the work done.

## Code conventions — frontend

### TypeScript is at maximal strictness

`tsconfig.app.json` enables `strict`, `noUncheckedIndexedAccess`,
`exactOptionalPropertyTypes`, `noPropertyAccessFromIndexSignature`,
`verbatimModuleSyntax`, `isolatedModules`, `noImplicitReturns`,
`noUnusedLocals`, `noUnusedParameters`, `noFallthroughCasesInSwitch`,
`noImplicitOverride`. Consequences:

- Use `import type` for type-only imports (`verbatimModuleSyntax` enforces it).
- Handle `T | undefined` from every indexed access.
- Never reach for `any` to bypass a shape mismatch — fix the shape.

### Svelte 5 runes, not stores

- State lives in singleton class instances in `src/lib/stores/*.svelte.ts`.
  Each file exports `class XState { ... = $state(...) }` and a singleton
  `export const x = new XState()`. Import by name:
  `import { queue } from "$lib/stores/queue.svelte"`.
- **Never use Svelte stores** (`writable`/`readable`/`derived`) — the codebase
  is fully rune-based. Never instantiate a second copy of a store singleton.
- Components use `<script lang="ts">` only. Svelte 5 surface: `onMount`,
  `$effect`, `$derived`, `{#key expr}`.
- Tauri IPC via `invoke<T>("command_name", { args })` from
  `@tauri-apps/api/core`. Events via `listen<Payload>("event_name", cb)` from
  `@tauri-apps/api/event`. Lazy listener setup uses a memoized
  `private listening: Promise<void> | null` pattern.

### Imports & aliases

- Use the **`$lib` alias** (`import { x } from "$lib/stores/queue.svelte"`) for
  everything under `src/lib/`. Avoid `../../` relative paths. The alias is
  wired in both `vite.config.ts` and `tsconfig.app.json` — keep them in sync
  if you touch paths.
- Icons are imported **one per file**:
  `import Zap from "lucide-svelte/icons/zap"`.

### Styling — DaisyUI on Tailwind 4, dark-only

- The custom DaisyUI theme `orangeloaf` is defined entirely in `src/app.css`
  via `@plugin "daisyui/theme" { ... }`. There is **no** `tailwind.config.js`
  or `postcss.config.js` — Tailwind 4 is CSS-first.
- App is **dark-only**. `index.html` sets `<html data-theme="orangeloaf">`.
  No light variant exists.
- Use DaisyUI utility classes directly (`bg-base-200`, `text-primary`,
  `btn`, `tooltip`, etc.) plus Tailwind utilities. A custom Tailwind theme var
  `--color-hairline: rgba(255,255,255,0.07)` powers `border-hairline` for
  hairline borders — **no drop shadows** except on floating overlays.
- Single rationed accent `#5e6ad2`, used only for primary actions, active nav,
  focus rings, and progress fill.
- Density: `<html data-density="compact">` shrinks root font to 14px (Settings
  > Appearance). Don't hardcode sizes that ignore this.
- Canonical spec: `design/design-system.md`. Mockups: `design/mockups/`
  (`convert-screen.html`/`.png`, `queue-screen.html`/`.png`). Match them when
  touching those screens.
- For UI work that needs design thinking, the `.claude/skills/superdesign/`
  skill is available — load it before implementing UI that warrants it.

### i18n — Paraglide

- Add **every new user-visible string** to **all five** `messages/{en,es,fr,de,ja}.json`
  files as a flat key. Values are plain strings (inlang-message-format).
- Call sites: `import { m } from "$lib/i18n.svelte"` then `m.your_key()` in
  template. The functions are pure (not reactive).
- Live locale switching: `App.svelte` wraps the whole tree in
  `{#key i18n.locale}` so Svelte remounts the subtree and every `m.xxx()`
  call re-evaluates. Most components inherit this for free — only wrap again
  if you add a subtree outside the App root.
- `i18n.svelte.ts` re-exports `m`, `locales`, `Locale`, and an `I18nState`
  class with `locale: Locale = $state(getLocale())` and a `setLocale` that
  calls paraglide's `setLocale(loc, { reload: false })` then bumps the rune.
- `Config.language` is persisted and reapplied on startup by an `$effect` in
  `App.svelte`.

### Tests

- Frontend tests: Vitest, `src/**/*.test.ts`, node environment. Only
  `src/lib/crop.test.ts` exists today (pure-function crop math). When adding
  tests, prefer pure-function extraction (like `crop.ts`) so logic is testable
  without DOM.

## Code conventions — Rust (src-tauri/)

### Lints

- `unsafe_code = "forbid"` — **never** use `unsafe`, even for env-var mutation
  under edition 2024 (the codebase explicitly avoids it; see the doc comment
  in `queue.rs`).
- Clippy `pedantic` is warn-by-default; CI runs `cargo clippy --all-targets
-- -D warnings`, so any warning fails the build.
- Allowed clippy lints: `module_name_repetitions`, `must_use_candidate`.
- On `#[tauri::command]` functions, add `#[allow(clippy::needless_pass_by_value)]`
  (Tauri only accepts owned `State<'_, T>`, not `&State`). This is the
  established pattern — match it.
- `rustfmt.toml`: `edition = "2024"`. Run `just fmt` (or `cargo fmt`) before
  committing; CI runs `cargo fmt --check`.

### IPC types — keep Rust and TS in sync

- Every IPC struct carries `#[serde(rename_all = "camelCase")]`. Tagged enums
  use `#[serde(tag = "name", rename_all = "camelCase")]` (see `EncoderKind`,
  `JobStatus { tag = "state" }`).
- The frontend mirror types live in `src/lib/*.ts` (e.g. `filters.ts` mirrors
  `src-tauri/src/filters.rs` — the file header says so explicitly). When you
  change a Rust IPC shape, update the TS mirror **in the same change**.
- The README calls out a past serde casing bug as a failure mode — don't
  reintroduce it. There's a pinned roundtrip test
  (`tagged_enum_serializes_with_the_expected_camel_case_tag_values`) that
  exists precisely to prevent this.

### Commands

- `#[tauri::command]` wrappers should be thin. Extract pure logic into
  `fn xxx_impl<R: Runtime>(...)` so tests can call it with a `MockRuntime`
  handle. (Several modules already do this; the pattern is documented inline
  with comments like "Extracted from the `#[tauri::command]` wrapper ...
  specifically so tests can call it with a `MockRuntime` handle".)
- App state: `AppState { Mutex<Config/Queue/Profiles/History>,
running_cancel: Mutex<Option<(String, oneshot::Sender<()>)>> }`, managed by
  Tauri. Dev-deps enable `tauri = { features = ["test"] }` so
  `tauri::test::mock_app()` provides a mock runtime — app code stays generic
  over `<R: Runtime>` without test-only generics leaking into production.

### File I/O

- Persisted state lives in the per-user data dir from
  `directories::ProjectDirs::from("dev", "orangeloaf", "OrangeLoaf")`,
  overridable via `ORANGELOAF_*` env vars.
- Atomic writes: write a uuid-named `.tmp` file then rename — never leave a
  truncated `queue.json` mid-crash. Match this when adding new persisted state.

### Tests

- Inline `#[cfg(test)] mod tests { ... }` in every module. Test names are
  **long snake_case sentences describing the property** (e.g.
  `reorder_queued_job_moving_forward_lands_on_the_intended_slot_not_one_past_it`).
- Tests needing real ffmpeg/ffprobe/hardware use
  `#[ignore = "requires system ffmpeg"]` (or `"... ffprobe"`,
  `"... libsvtav1"`, `"... macOS VideoToolbox hardware encoding"`) — they run
  via `cargo test -- --ignored` locally and are skipped by CI. Test doc
  comments describe what property they verify and why a mocked test couldn't.
- Per-test-module helpers: `fn test_job(source, output) -> Job` and
  `fn mock_state() -> AppState`. Reuse them; don't duplicate fixtures.
- Fixtures for real-media integration tests live in `src-tauri/tests/fixtures/`
  (`hdr10.mkv`, `multitrack.mkv`, `sdr_10bit.mp4`, `sdr_8bit.mp4`).

### Encoders

- `EncoderProfile` trait with
  `fn build_steps(&self, ctx: &EncodeContext) -> Vec<EncodeStep>`.
- `EncoderKind` is a tagged enum (one variant per encoder); a generic
  `impl EncoderProfile for EncoderKind` delegates to the inner settings'
  `build_steps`. Per-encoder settings structs are
  `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]` +
  `#[serde(rename_all = "camelCase")]` with an explicit `impl Default`.
- Shared argv builders (`build_prefix`, `build_suffix`,
  `build_two_pass_steps`) live in `encoders/common.rs`. HDR10 metadata
  re-injection is in `encoders/hdr.rs`. Audio/subtitle track selection is in
  `encoders/tracks.rs`. Add new encoders as a new variant + settings struct +
  `encoders/<name>.rs` module.

## Comments & docs

- **Do not add comments unless asked** (default code-style rule). When you do,
  match the existing voice: terse, technical, explains _why_ (not _what_).
- Rust doc comments use `///` with `# Errors` and `# Panics` sections where
  relevant. Inline references to `notes/plan.md` milestones (e.g.
  "(notes/plan.md M12)") and section numbers are common — keep that style.
- Frontend comments explain non-obvious cross-file parity (e.g. "Mirrors
  src-tauri/src/filters.rs's Crop/FilterSpec wire shape exactly" in
  `src/lib/filters.ts`).

## Prettier / formatting scope

`.prettierignore` skips: `dist/`, `src-tauri/`, `node_modules/`, `public/`,
`design/`, `notes/plan.md`, `project.inlang/`, `.claude/`, `src-tauri/gen/`.
Prettier defaults: 2-space indent, 80-col print width, double quotes,
trailing comma `all`, semicolons always. ESLint is type-aware
(`strictTypeChecked` + `stylisticTypeChecked` via `projectService`); the
eslint config ignores generated `src/paraglide/**/*.js` and exempts the
generated `*.d.ts` from style rules.

## Commit conventions

- **Imperative mood**, capitalized first word, one-line summary. Examples:
  `Fix CI: stop Prettier checking tool-managed generated files`,
  `Add Load/Save Queue`, `Rebuild Convert view to match the design mockup`.
- **No** conventional-commits prefixes (`feat:`/`fix:`/`chore:`).
- Milestone-prefixed commits reference `notes/plan.md` (e.g. `Add M14:
packaging config + CI workflows`). Use this only if your change maps to a
  milestone.
- Commit only what you intend; never commit secrets. Don't update git config,
  skip hooks, force-push, or create empty commits unless explicitly asked.
- Run `just check` before committing so you don't push red.

## Don't

- Don't bundle FFmpeg/FFprobe. The user supplies their own.
- Don't add `unsafe` to Rust. It's `forbid`den.
- Don't use Svelte stores (`writable`/`readable`/`derived`). Runes only.
- Don't reach for `any` in TS to bypass a shape mismatch. Fix the shape.
- Don't invoke bare `tsc` or `eslint` — go through `bun run` so the paraglide
  prebuild hooks fire.
- Don't assume a light theme exists. The app is dark-only.
- Don't use drop shadows except on floating overlays. Use hairline borders.
- Don't skip adding new i18n strings to all five `messages/*.json` files.
- Don't change a Rust IPC shape without updating the TS mirror in `src/lib/`.
- Don't commit `src/paraglide/` — it's gitignored and regenerated.
