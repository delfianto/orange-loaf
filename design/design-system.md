# OrangeLoaf — Design System

## Product context

OrangeLoaf is a desktop video transcoding application (Tauri + Rust + Svelte 5)
that wraps FFmpeg and ~30 software/hardware encoder backends (x264, x265, SVT-AV1, AOM AV1,
rav1e, VP9, VVC, NVENC/NVEncC, QSVEncC, VCEEncC, VAAPI, VideoToolbox). Its whole reason for
being is fixing the failure mode common to older tools in this space — **visual noise with
zero hierarchy**: a 10-tab horizontal strip of dense option grids, tiny unlabeled checkboxes,
inconsistent spacing, and no sense of what matters right now. Users are power users
(video hobbyists, archivists, self-hosted media server operators) who batch-convert and
compress video files, tune codec/quality/HDR settings, queue multiple jobs, and monitor
encode progress — often leaving it running for hours unattended. The redesign must read as
calm, premium, and information-dense-but-organized: a professional tool, not a settings dump.

## Style anchor

Primary style source: SuperDesign library prompt **"Dashboard UI: Dark Linear-Style Developer
Issue Tracker"** (slug `dashboard-ui-dark-linear-style-developer-issue-tracker`). Adopting its
DNA directly — dark cool-indigo product-UI minimalism, hierarchy from size/weight rather than
color, flat/border-only surfaces, single rationed accent. Do not blend in any other library
style. The only addition on top of that anchor is a monospace type layer for genuinely
technical/tabular content (ffmpeg command previews, codec ids, log output) — the anchor system
already uses mono for issue ids, so this is an extension of its own vocabulary, not a
competing style.

## Branding

**Canvas (near-black, three elevation steps):**
- App background: `#08090a`
- Sidebar / rail background: `#0c0d0f`
- Main content background: `#0b0c0e`
- Raised surface (cards, popovers, modals): `#111318`
- Hairline border/divider: `rgba(255,255,255,0.07)` — never a drop shadow

**Type:**
- UI typeface: **Inter** — the only UI typeface, hierarchy from size/weight only
  - Primary text: `#f4f5f8` (cool near-white)
  - Secondary text: `#b9bdc6`
  - Muted/meta text (ids, timestamps, hints): `#7d828c`
  - Never below 12px
- Technical/mono typeface: **JetBrains Mono** — reserved for ffmpeg command strings, codec
  parameter values, file paths, durations/timecodes, log/debug output. Rendered at 12–13px.

**Accent (single, rationed):**
- Primary accent: `#5e6ad2` (indigo/violet) — used ONLY for: the one primary filled button per
  screen (Convert / Add to Queue / Save), the active nav-item tint, focus rings, and the
  in-progress portion of progress bars. Never used decoratively.

**Semantic status colors (small, confined to icons/rings/pills — never full-surface fills):**
- Queued / pending: `#7d828c` (muted gray, dashed ring)
- Encoding / in progress: `#f2c94c` (amber, half-filled ring + animated progress bar)
- Completed / success: `#4ade80` (green, filled check)
- Error / failed: `#eb5757` (red)
- Paused: `#5e6ad2` (indigo outline)
- HDR/codec capability badges (HDR10, HDR10+, Dolby Vision, 10-bit, 2-Pass, GPU): small
  rounded label-pill chips, colored dot + muted text on a faint tint of the same hue — same
  pattern as the anchor's label pills.

**Shape & elevation:**
- Border radius: tight, 6–8px on controls/cards, 4px on chips/badges. Never fully square,
  never pill-shaped except status/label chips.
- No drop shadows anywhere in the base UI. Surfaces are separated by hairlines and background
  elevation steps only. A very soft shadow (`0 8px 24px rgba(0,0,0,0.35)`) is permitted only
  on floating overlays (modals, dropdown menus, the large-preview popout) that sit above the
  whole app shell.
- Density: rows ~36–40px, section padding 16–20px, generous 12px gaps between grouped
  controls — dense but never cramped (this is the single biggest fix over the legacy UI).

**Motion:**
- Fast, mechanical, no bounce: 120–180ms ease-out for hover/active/focus state changes.
- Progress bars and the live command-preview text animate smoothly (no layout jumps).
- Toasts (job completion, errors) slide in from the bottom-right and auto-dismiss.
- Encoding-in-progress rows get a subtle pulsing amber ring on the status icon — reuse the
  anchor's "pulse-ring" idea sparingly, only for the single row currently encoding.

## Layout structure

App shell = two-pane, fixed ~72px icon-rail sidebar (not the anchor's full 240px labeled
sidebar — this app has 4–5 top-level destinations, not a workspace/teams hierarchy, so a
compact icon rail with tooltips fits better) + main content column:

- **Icon rail** (`#0c0d0f`, hairline right border): app mark at top; nav icons for
  **Convert**, **Queue** (with a live count badge while jobs are active), **History**,
  **Profiles**; **Settings** pinned to the bottom. Active item gets the indigo tint fill used
  by the anchor's active-item pattern.
- **Convert view** (primary/default screen): a header bar (source file path + Change Source +
  encoder dropdown + profile dropdown, primary "Add to Queue" and "Convert" buttons on the
  right, exactly one of which is the solid indigo filled button); below it a two-column
  workspace — left/larger: video preview with scrubber, crop overlay, and a compact
  Size/Trim/Crop/Rotate strip; right/narrower: a **vertical section list** (Quality, Audio,
  Subtitles, Filters, HDR/Advanced, Command Preview) replacing the legacy horizontal 10-tab
  strip — one section visibly expanded at a time, others collapsed to a single labeled row,
  so the user's attention goes to one coherent group of controls instead of a wall of them.
- **Queue view**: the anchor's grouped-list pattern repurposed — jobs grouped by status
  (Encoding / Queued / Completed / Failed), each row a status ring, filename, codec badge
  pills, a slim progress bar + ETA, and a hover action menu (pause/cancel/remove/retry/open).
- **History / Profiles / Settings**: simpler single-column forms and lists using the same
  tokens; Settings groups binary paths, GPU/hardware-encoder detection status, language,
  and update checks into labeled sections with hairline dividers, mirroring the anchor's
  inspector-sidebar form density.
- Responsive floor is desktop-only (this is a desktop app via Tauri, min window ~1100×700);
  no mobile breakpoint work needed, but the settings section list collapses to single column
  below ~1280px content width.

## Project requirements

- Every screen must be buildable with **DaisyUI 5 (Tailwind CSS 4) components** on the Svelte
  side — favor DaisyUI's existing primitives (btn, input, select, toggle, radio, tabs, menu,
  progress, badge, tooltip, modal, dropdown) restyled to these tokens over bespoke markup.
- Full feature parity with the legacy app's control inventory is required before any screen
  is considered done (see the implementation plan's functional inventory) — this is a
  redesign of information architecture and visual language, not a feature cut.
- Accessibility: maintain WCAG AA contrast minimum for all text/icon-on-background pairs
  (the anchor's near-white-on-near-black and semantic hues already clear this comfortably).
