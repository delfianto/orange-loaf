<script lang="ts">
  import { onMount } from "svelte";
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import Copy from "lucide-svelte/icons/copy";
  import Minus from "lucide-svelte/icons/minus";
  import Plus from "lucide-svelte/icons/plus";
  import RotateCw from "lucide-svelte/icons/rotate-cw";
  import RotateCcw from "lucide-svelte/icons/rotate-ccw";
  import CropIcon from "lucide-svelte/icons/crop";
  import Video from "lucide-svelte/icons/video";
  import { video } from "$lib/stores/video.svelte";
  import {
    encode,
    ENCODER_NAMES,
    X265_PRESETS,
    X264_PRESETS,
    AUDIO_CODECS,
    type EncoderName,
    type AudioTrackSelection,
    type AudioAction,
  } from "$lib/stores/encode.svelte";
  import { sanitizeCrop } from "$lib/crop";
  import type { Crop } from "$lib/filters";
  import { queue } from "$lib/stores/queue.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";
  import { config } from "$lib/stores/config.svelte";
  import { profiles } from "$lib/stores/profiles.svelte";
  import AccordionSection from "$lib/components/AccordionSection.svelte";
  import CropOverlay from "$lib/components/CropOverlay.svelte";
  import TrackRow from "$lib/components/TrackRow.svelte";

  onMount(() => {
    void profiles.refresh();
  });

  let selectedProfileId = $state("");

  // Mirrors ProfilesView.svelte's own `applyProfile` exactly, minus the
  // navigation (we're already on Convert) — this picker is a one-shot
  // action, not a persistent "currently applied profile" selector (nothing
  // in `encode` state tracks that concept), so the select resets to its
  // placeholder immediately after applying.
  async function onProfilePickerChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): Promise<void> {
    const id = event.currentTarget.value;
    selectedProfileId = "";
    const profile = profiles.profiles.find((p) => p.id === id);
    if (!profile) {
      return;
    }
    const sourceAudioTracks = video.info?.audioTracks ?? [];
    const applied = await profiles.apply(profile.id, sourceAudioTracks);
    encode.settings = applied.settings;
    encode.filters = applied.filters;
    encode.audioTracks = applied.audioTracks;
  }

  type SectionId =
    "quality" | "audio" | "subtitles" | "filters" | "hdr" | "command";
  let openSection: SectionId = $state("quality");

  // HDR/Advanced is still a stub — HDR10 metadata is preserved automatically,
  // so there's nothing to configure yet (the collapsed row still surfaces the
  // detected HDR10 signaling via a summary + badge).
  const HDR_NOTE =
    "HDR10 mastering-display/light-level metadata is preserved automatically for HEVC (x265) and AV1 (SVT-AV1) outputs — there's nothing to configure here yet. HDR10+, tonemap, and the other advanced controls are still coming.";

  $effect(() => {
    // A new source (or a cleared one) always gets a fresh, all-enabled
    // track selection — see `resetTracksFrom`'s doc comment for why prior
    // choices aren't carried over.
    encode.resetTracksFrom(video.info);
  });

  $effect(() => {
    const path = video.path;
    // `$state.snapshot` (rather than a plain reference read) is what makes
    // this effect actually depend on nested field changes — CRF, crop
    // corners, trim times, etc. A shallow read of `encode.settings` only
    // depends on *that field being reassigned wholesale*, which slider/
    // input edits never do; they mutate a nested property in place, which
    // a shallow read silently misses (verified directly against this
    // Svelte version before relying on it, not assumed from docs).
    const settings = $state.snapshot(encode.settings);
    const filters = $state.snapshot(encode.filters);
    const trim = $state.snapshot(encode.trim);
    const audioTracks = $state.snapshot(encode.audioTracks);
    const subtitleTracks = $state.snapshot(encode.subtitleTracks);
    const videoInfo = video.info ? $state.snapshot(video.info) : null;
    void encode.refreshPreview(
      path,
      settings,
      filters,
      trim,
      audioTracks,
      subtitleTracks,
      videoInfo,
    );
  });

  function sourceName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function formatTime(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    const mm = String(m).padStart(h > 0 ? 2 : 1, "0");
    const ss = String(sec).padStart(2, "0");
    return h > 0 ? `${String(h)}:${mm}:${ss}` : `${mm}:${ss}`;
  }

  function onScrub(event: Event & { currentTarget: HTMLInputElement }): void {
    void video.seekTo(Number(event.currentTarget.value));
  }

  function isEncoderName(value: string): value is EncoderName {
    return ENCODER_NAMES.some((encoder) => encoder.id === value);
  }

  function onEncoderChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const { value } = event.currentTarget;
    if (isEncoderName(value)) {
      encode.selectEncoder(value);
    }
  }

  function onTuneChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    if (encode.settings.name === "x265" || encode.settings.name === "x264") {
      encode.settings.tune = event.currentTarget.value || null;
    }
  }

  function onAudioActionKindChange(
    selection: AudioTrackSelection,
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    selection.action =
      event.currentTarget.value === "convert"
        ? { action: "convert", codec: "aac", quality: 4, downmix: null }
        : { action: "copy" };
  }

  function onDownmixChange(
    action: Extract<AudioAction, { action: "convert" }>,
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const raw = event.currentTarget.value;
    action.downmix = raw === "" ? null : Number(raw);
  }

  // The strip's "Open Crop Tool" toggles crop on/off: enabling seeds a
  // 10%-inset rectangle (the preview's drag handles then refine it), disabling
  // clears it. Exact pixel values remain editable in the Filters section while
  // crop is active.
  function toggleCropTool(): void {
    if (encode.filters.crop) {
      encode.filters.crop = null;
      return;
    }
    if (!video.info) {
      return;
    }
    const { width, height } = video.info;
    encode.filters.crop = sanitizeCrop(
      {
        top: Math.round(height * 0.1),
        bottom: Math.round(height * 0.1),
        left: Math.round(width * 0.1),
        right: Math.round(width * 0.1),
      },
      width,
      height,
    );
  }

  // Re-sanitizes on blur/enter so a hand-typed value (odd, out-of-range,
  // negative) always settles back into a crop ffmpeg can run — the same
  // rule the drag tool enforces on every move. Live typing itself is left
  // alone (via `bind:value`) rather than sanitized on every keystroke,
  // which would fight the user's cursor mid-edit.
  function commitCrop(): void {
    if (!encode.filters.crop || !video.info) {
      return;
    }
    encode.filters.crop = sanitizeCrop(
      encode.filters.crop,
      video.info.width,
      video.info.height,
    );
  }

  // Parses the strip's free-text "Custom" resolution box: accepts "1920x1080",
  // "1920×1080", "1920 1080", or a lone width. Blank clears both dimensions
  // (auto/source). Committed on change (not per keystroke) so a half-typed
  // value never fights the cursor.
  function onCustomResolutionChange(
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    const raw = event.currentTarget.value.trim();
    if (raw === "") {
      encode.filters.scaleWidth = null;
      encode.filters.scaleHeight = null;
      return;
    }
    const parts = raw.split(/[×xX,\s]+/).filter((part) => part !== "");
    const w = parts[0] != null ? Math.round(Number(parts[0])) : NaN;
    const h = parts[1] != null ? Math.round(Number(parts[1])) : NaN;
    encode.filters.scaleWidth = Number.isFinite(w) && w > 0 ? w : null;
    encode.filters.scaleHeight = Number.isFinite(h) && h > 0 ? h : null;
  }

  // Flip is a single select (mockup parity) over the two independent
  // horizontal/vertical booleans; "both" keeps the underlying capability of
  // combining them (a 180° mirror) rather than dropping it.
  function onFlipChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const value = event.currentTarget.value;
    encode.filters.horizontalFlip = value === "h" || value === "both";
    encode.filters.verticalFlip = value === "v" || value === "both";
  }

  interface ResolutionPreset {
    key: string;
    label: string;
    width: number | null;
    height: number | null;
  }

  const RESOLUTION_PRESETS: ResolutionPreset[] = [
    { key: "auto", label: "Auto (Source)", width: null, height: null },
    { key: "4k", label: "3840×2160 (4K)", width: 3840, height: 2160 },
    { key: "1440p", label: "2560×1440 (1440p)", width: 2560, height: 1440 },
    { key: "1080p", label: "1920×1080 (1080p)", width: 1920, height: 1080 },
    { key: "720p", label: "1280×720 (720p)", width: 1280, height: 720 },
    { key: "480p", label: "854×480 (480p)", width: 854, height: 480 },
  ];

  // Derived (not its own field) so a resolution typed directly into the
  // width/height inputs below that happens to match a preset is reflected
  // here too, and one that doesn't just reads as "Custom" — there's nothing
  // to keep in sync since the inputs stay the single source of truth.
  const resolutionPreset = $derived(
    RESOLUTION_PRESETS.find(
      (p) =>
        p.width === encode.filters.scaleWidth &&
        p.height === encode.filters.scaleHeight,
    )?.key ?? "custom",
  );

  // Reflects the explicit W×H only when it doesn't match a named preset, so
  // the Custom box reads empty while a preset is selected (mockup behaviour).
  const customResolution = $derived(
    resolutionPreset === "custom"
      ? `${encode.filters.scaleWidth ? String(encode.filters.scaleWidth) : ""}×${encode.filters.scaleHeight ? String(encode.filters.scaleHeight) : ""}`
      : "",
  );

  const flipValue = $derived(
    encode.filters.horizontalFlip && encode.filters.verticalFlip
      ? "both"
      : encode.filters.horizontalFlip
        ? "h"
        : encode.filters.verticalFlip
          ? "v"
          : "none",
  );

  function onResolutionPresetChange(
    event: Event & { currentTarget: HTMLSelectElement },
  ): void {
    const preset = RESOLUTION_PRESETS.find(
      (p) => p.key === event.currentTarget.value,
    );
    if (preset) {
      encode.filters.scaleWidth = preset.width;
      encode.filters.scaleHeight = preset.height;
    }
  }

  function rotateClockwise(): void {
    encode.filters.rotateDegrees = (encode.filters.rotateDegrees + 90) % 360;
  }

  function rotateCounterClockwise(): void {
    encode.filters.rotateDegrees = (encode.filters.rotateDegrees + 270) % 360;
  }

  function onTrimInput(
    field: "startTime" | "endTime",
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    const raw = event.currentTarget.value;
    encode.trim[field] = raw === "" ? null : Number(raw);
  }

  function onConvertClick(): void {
    if (!video.path || !video.info || encode.status?.state === "running") {
      return;
    }
    void encode.start(video.path, video.info.durationSecs, video.info);
  }

  async function onAddToQueueClick(): Promise<void> {
    if (!video.path || !video.info) {
      return;
    }
    await queue.enqueue(
      video.path,
      video.info.durationSecs,
      $state.snapshot(encode.settings),
      $state.snapshot(encode.filters),
      $state.snapshot(encode.trim),
      $state.snapshot(encode.audioTracks),
      $state.snapshot(encode.subtitleTracks),
      video.info,
    );
    navigation.current = "queue";
  }

  function copyCommand(): void {
    void navigator.clipboard.writeText(
      encode.commandPreview.map((step) => step.join(" ")).join("\n\n"),
    );
  }

  const isRunning = $derived(encode.status?.state === "running");

  // --- Quality panel (mockup parity) --------------------------------------

  // CRF ceiling differs by codec: SVT-AV1 spans 0–63, x264/x265 span 0–51.
  const crfMax = $derived(encode.settings.name === "svtAv1" ? 63 : 51);

  // The output vertical resolution the encode will actually produce, shown as
  // a hint next to the CRF value. Prefer an explicit scale height; else derive
  // one from an explicit scale width against the source aspect ratio; else
  // fall back to the source height. Null (hidden) until a source is probed.
  const outputHeight = $derived.by(() => {
    const h = encode.filters.scaleHeight;
    if (h != null) {
      return h;
    }
    const w = encode.filters.scaleWidth;
    const info = video.info;
    if (w != null && info) {
      return Math.round((w * info.height) / info.width);
    }
    return info?.height ?? null;
  });

  // x265 is the only encoder exposing a pixel-format (bit-depth) choice; the
  // header pill mirrors the Bit Depth / Pixel Format select below it.
  const bitDepthLabel = $derived(
    encode.settings.name === "x265"
      ? encode.settings.pixFmt.includes("10")
        ? "10-bit"
        : "8-bit"
      : null,
  );

  // Both x265 and x264 expose ffmpeg's classic -pass 2-pass mechanism; the
  // pill and toggle only exist for those two (SVT-AV1/VideoToolbox have no
  // such field). See src-tauri/src/encoders/x265.rs — two_pass only splits
  // into two steps when a target bitrate is also set, so the toggle is
  // disabled (and the hint changes) while in CRF mode.
  const supportsTwoPass = $derived(
    encode.settings.name === "x265" || encode.settings.name === "x264",
  );
  const twoPassOn = $derived(
    (encode.settings.name === "x265" || encode.settings.name === "x264") &&
      encode.settings.twoPass,
  );
  const crfMode = $derived(
    "targetBitrateKbps" in encode.settings &&
      encode.settings.targetBitrateKbps == null,
  );

  function stepCrf(delta: number): void {
    if ("crf" in encode.settings) {
      encode.settings.crf = Math.min(
        crfMax,
        Math.max(0, encode.settings.crf + delta),
      );
    }
  }

  // Switching modes preserves the last bitrate (defaulting to 4000 kbps the
  // first time) — the same contract the old CRF/Bitrate join buttons had.
  function setBitrateMode(on: boolean): void {
    if (!("targetBitrateKbps" in encode.settings)) {
      return;
    }
    encode.settings.targetBitrateKbps = on
      ? (encode.settings.targetBitrateKbps ?? 4000)
      : null;
  }

  const PILL_ACTIVE = "border-primary/20 bg-primary/10 text-primary";
  const PILL_MUTED = "border-hairline bg-white/5 text-base-content/50";
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <div class="flex min-w-0 items-center gap-4">
      {#if video.path}
        <div class="flex min-w-0 items-center gap-3">
          <Video class="text-base-content/50 h-5 w-5 shrink-0" />
          <div class="flex min-w-0 flex-col">
            <span class="truncate text-sm leading-tight font-medium"
              >{sourceName(video.path)}</span
            >
            <span
              class="text-base-content/50 truncate text-[11px] tracking-wider uppercase"
              >Source File • {video.path}</span
            >
          </div>
        </div>
        <button
          type="button"
          class="btn btn-ghost btn-sm shrink-0"
          onclick={() => video.choose()}
        >
          <FolderOpen class="h-4 w-4" />
          Change Source
        </button>
      {:else}
        <span class="text-base-content/50 text-sm">No source selected</span>
        <button
          type="button"
          class="btn btn-ghost btn-sm"
          onclick={() => video.choose()}
        >
          <FolderOpen class="h-4 w-4" />
          Choose Source
        </button>
      {/if}
    </div>
    <div class="flex shrink-0 items-center gap-3">
      <div class="flex items-center gap-2">
        <select
          class="select select-sm"
          value={encode.settings.name}
          onchange={onEncoderChange}
        >
          {#each ENCODER_NAMES as enc (enc.id)}
            <option value={enc.id}>{enc.label}</option>
          {/each}
        </select>
        <select
          class="select select-sm"
          bind:value={selectedProfileId}
          onchange={onProfilePickerChange}
          disabled={profiles.profiles.length === 0}
          aria-label="Apply profile"
        >
          <option value="" disabled>
            {profiles.profiles.length === 0 ? "No profiles" : "Apply Profile…"}
          </option>
          {#each profiles.profiles as profile (profile.id)}
            <option value={profile.id}>{profile.name}</option>
          {/each}
        </select>
      </div>
      <div class="bg-hairline mx-1 h-6 w-px"></div>
      <div class="flex items-center gap-2">
        <button
          type="button"
          class="btn btn-ghost btn-sm"
          disabled={!video.path}
          onclick={() => {
            void onAddToQueueClick();
          }}
        >
          Add to Queue
        </button>
        <button
          type="button"
          class="btn btn-primary btn-sm"
          disabled={!video.path || isRunning}
          onclick={onConvertClick}
        >
          {isRunning ? "Encoding…" : "Convert"}
        </button>
      </div>
    </div>
  </header>

  {#if video.error}
    <div class="alert alert-error mx-6 mt-4 text-sm">{video.error}</div>
  {/if}

  {#if encode.status}
    <div class="border-hairline border-b px-6 py-3">
      {#if encode.status.state === "running"}
        <div class="flex items-center gap-3">
          <progress
            class="progress progress-primary flex-1"
            value={encode.progress?.percent ?? 0}
            max="100"
          ></progress>
          <span
            class="text-base-content/50 w-40 shrink-0 text-right font-mono text-xs"
          >
            {encode.progress?.percent != null
              ? `${encode.progress.percent.toFixed(0)}%`
              : "starting…"}
            {#if encode.progress?.speed != null}
              · {encode.progress.speed.toFixed(1)}x
            {/if}
          </span>
        </div>
      {:else if encode.status.state === "completed"}
        <p class="text-success text-sm">Encode complete.</p>
      {:else if encode.status.state === "failed"}
        <p class="text-error whitespace-pre-wrap font-mono text-xs">
          {encode.status.message}
        </p>
      {/if}
    </div>
  {/if}

  <div
    class="grid flex-1 grid-cols-[3fr_2fr] items-start gap-6 overflow-y-auto p-6"
  >
    <div class="flex min-w-0 flex-col gap-4">
      <div
        class="bg-base-300 border-hairline relative flex aspect-video items-center justify-center overflow-hidden rounded-lg border"
      >
        {#if video.probing}
          <Loader class="text-base-content/40 h-6 w-6 animate-spin" />
        {:else if video.thumbnail}
          <img
            src={video.thumbnail}
            alt="Preview frame"
            class="h-full w-full object-contain"
          />
          {#if video.thumbnailLoading}
            <div
              class="bg-base-300/60 absolute inset-0 flex items-center justify-center"
            >
              <Loader class="text-base-content/60 h-5 w-5 animate-spin" />
            </div>
          {/if}
        {:else}
          <p class="text-base-content/50 text-sm">
            {video.path
              ? "Preview not available yet"
              : "Choose a source to get started"}
          </p>
        {/if}
        {#if video.info && encode.filters.crop}
          {@const info = video.info}
          {@const crop = encode.filters.crop}
          <CropOverlay
            sourceWidth={info.width}
            sourceHeight={info.height}
            {crop}
            onChange={(next: Crop) => (encode.filters.crop = next)}
          />
        {/if}
        {#if video.info}
          <div class="pointer-events-none absolute top-3 left-3 flex gap-2">
            <span
              class="rounded-sm bg-black/60 px-2 py-1 font-mono text-[10px] text-white/80 backdrop-blur"
              >{video.info.width} × {video.info.height}</span
            >
            <span
              class="rounded-sm bg-black/60 px-2 py-1 font-mono text-[10px] text-white/80 uppercase backdrop-blur"
              >{video.info.codecName}</span
            >
            <span
              class="rounded-sm bg-black/60 px-2 py-1 font-mono text-[10px] text-white/80 backdrop-blur"
              >{video.info.bitDepth}-bit</span
            >
            {#if video.info.hdr10}
              <span
                class="bg-warning/20 text-warning rounded-sm px-2 py-1 font-mono text-[10px] backdrop-blur"
                >HDR10</span
              >
            {/if}
          </div>
        {/if}
      </div>

      {#if video.info}
        <div class="flex items-center gap-3">
          <input
            type="range"
            min="0"
            max={video.info.durationSecs}
            step="0.1"
            value={video.scrubTime}
            onchange={onScrub}
            class="range range-primary range-xs flex-1"
          />
          <span class="text-base-content/50 shrink-0 font-mono text-xs">
            {formatTime(video.scrubTime)} / {formatTime(
              video.info.durationSecs,
            )}
          </span>
        </div>
      {/if}

      <!-- Transform strip: the mockup's Resolution / Rotate / Flip / Crop-tool
           row sits directly under the preview (these were previously buried in
           the Filters accordion). -->
      <div class="grid grid-cols-4 gap-4">
        <div class="flex flex-col gap-2">
          <span
            class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >Resolution</span
          >
          <div class="flex gap-1">
            <select
              class="select select-sm flex-1"
              value={resolutionPreset}
              onchange={onResolutionPresetChange}
            >
              {#each RESOLUTION_PRESETS as preset (preset.key)}
                <option value={preset.key}>{preset.label}</option>
              {/each}
              <option value="custom" disabled={resolutionPreset !== "custom"}
                >Custom</option
              >
            </select>
            <input
              type="text"
              placeholder="Custom"
              class="input input-sm w-24 font-mono"
              value={customResolution}
              onchange={onCustomResolutionChange}
            />
          </div>
        </div>

        <div class="flex flex-col gap-2">
          <span
            class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >Rotate</span
          >
          <div
            class="border-hairline bg-base-300 flex h-8 overflow-hidden rounded-md border"
          >
            <button
              type="button"
              class="border-hairline flex flex-1 items-center justify-center border-r hover:bg-white/5"
              aria-label="Rotate 90° counter-clockwise"
              onclick={rotateCounterClockwise}
            >
              <RotateCcw class="h-3.5 w-3.5" />
            </button>
            <div
              class="border-hairline flex flex-[1.5] items-center justify-center border-r font-mono text-xs"
            >
              {encode.filters.rotateDegrees}°
            </div>
            <button
              type="button"
              class="flex flex-1 items-center justify-center hover:bg-white/5"
              aria-label="Rotate 90° clockwise"
              onclick={rotateClockwise}
            >
              <RotateCw class="h-3.5 w-3.5" />
            </button>
          </div>
        </div>

        <div class="flex flex-col gap-2">
          <span
            class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >Flip</span
          >
          <select
            class="select select-sm"
            value={flipValue}
            onchange={onFlipChange}
          >
            <option value="none">No Flip</option>
            <option value="h">Horizontal</option>
            <option value="v">Vertical</option>
            <option value="both">Both (H+V)</option>
          </select>
        </div>

        <div class="flex flex-col gap-2">
          <span
            class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >Tool</span
          >
          <button
            type="button"
            class="btn btn-sm gap-2"
            class:btn-primary={encode.filters.crop != null}
            disabled={!video.info}
            onclick={toggleCropTool}
          >
            <CropIcon class="h-4 w-4" />
            Open Crop Tool
          </button>
        </div>
      </div>
    </div>

    <!-- Single bordered card: a hairline background shows through the
         `gap-px` between the `bg-base-300` sections as 1px dividers, matching
         the design mockup's transcode-settings panel. -->
    <div
      class="bg-hairline border-hairline flex min-w-0 flex-col gap-px overflow-hidden rounded-lg border"
    >
      <AccordionSection
        title="Quality"
        open={openSection === "quality"}
        onToggle={() => (openSection = "quality")}
      >
        {#snippet headerRight()}
          {#if bitDepthLabel}
            <span
              class="rounded border px-2 py-0.5 text-[10px] font-medium tracking-tight uppercase {bitDepthLabel ===
              '10-bit'
                ? PILL_ACTIVE
                : PILL_MUTED}"
            >
              {bitDepthLabel}
            </span>
          {/if}
          {#if supportsTwoPass}
            <span
              class="rounded border px-2 py-0.5 text-[10px] font-medium tracking-tight uppercase {twoPassOn
                ? PILL_ACTIVE
                : PILL_MUTED}"
            >
              2-Pass
            </span>
          {/if}
        {/snippet}
        <div class="flex flex-col gap-5">
          {#if encode.settings.name === "x265" || encode.settings.name === "x264"}
            <div class="grid grid-cols-2 gap-4">
              <label class="form-control">
                <span
                  class="text-base-content/50 mb-1.5 text-[10px] font-semibold tracking-wider uppercase"
                  >Preset</span
                >
                <select
                  class="select select-sm"
                  bind:value={encode.settings.preset}
                >
                  {#each encode.settings.name === "x265" ? X265_PRESETS : X264_PRESETS as preset (preset)}
                    <option value={preset}>{preset}</option>
                  {/each}
                </select>
              </label>
              <label class="form-control">
                <span
                  class="text-base-content/50 mb-1.5 text-[10px] font-semibold tracking-wider uppercase"
                  >Tune</span
                >
                <select
                  class="select select-sm"
                  value={encode.settings.tune ?? ""}
                  onchange={onTuneChange}
                >
                  <option value="">Default</option>
                  {#if encode.settings.name === "x264"}
                    <option value="film">Film</option>
                  {/if}
                  <option value="grain">Grain</option>
                  <option value="animation">Animation</option>
                  <option value="fastdecode">Fast Decode</option>
                  <option value="zerolatency">Zero Latency</option>
                </select>
              </label>
            </div>
          {:else if encode.settings.name === "svtAv1"}
            <label class="form-control">
              <span
                class="text-base-content/50 mb-1.5 text-[10px] font-semibold tracking-wider uppercase"
                >Preset (speed)
                <span class="font-mono normal-case"
                  >{encode.settings.speed}</span
                ></span
              >
              <input
                type="range"
                min="0"
                max="13"
                step="1"
                bind:value={encode.settings.speed}
                class="range range-primary range-xs"
              />
            </label>
          {:else}
            <label class="form-control">
              <span
                class="text-base-content/50 mb-1.5 text-[10px] font-semibold tracking-wider uppercase"
                >Quality
                <span class="font-mono normal-case"
                  >{encode.settings.quality}</span
                > (higher is better)</span
              >
              <input
                type="range"
                min="0"
                max="100"
                step="1"
                bind:value={encode.settings.quality}
                class="range range-primary range-xs"
              />
            </label>
          {/if}

          {#if encode.settings.name === "x265"}
            <label class="form-control">
              <span
                class="text-base-content/50 mb-1.5 text-[10px] font-semibold tracking-wider uppercase"
                >Bit Depth / Pixel Format</span
              >
              <select
                class="select select-sm"
                bind:value={encode.settings.pixFmt}
              >
                <option value="yuv420p10le">10-bit: yuv420p10le</option>
                <option value="yuv420p">8-bit: yuv420p</option>
              </select>
            </label>
          {/if}

          {#if encode.settings.name === "x265" || encode.settings.name === "x264" || encode.settings.name === "svtAv1"}
            <div class="bg-hairline h-px"></div>

            <div class="flex flex-col gap-4">
              <div class="flex items-center justify-between gap-3">
                <div
                  class="bg-base-100 border-hairline flex rounded-lg border p-0.5"
                >
                  <button
                    type="button"
                    class="rounded-md px-4 py-1.5 text-[11px] font-semibold transition-colors {crfMode
                      ? 'bg-primary text-primary-content'
                      : 'text-base-content/50 hover:text-base-content'}"
                    onclick={() => {
                      setBitrateMode(false);
                    }}
                  >
                    CRF
                  </button>
                  <button
                    type="button"
                    class="rounded-md px-4 py-1.5 text-[11px] font-semibold transition-colors {!crfMode
                      ? 'bg-primary text-primary-content'
                      : 'text-base-content/50 hover:text-base-content'}"
                    onclick={() => {
                      setBitrateMode(true);
                    }}
                  >
                    Bitrate
                  </button>
                </div>

                {#if crfMode}
                  <div
                    class="bg-base-100 border-hairline flex items-center overflow-hidden rounded-md border"
                  >
                    <button
                      type="button"
                      class="flex h-8 items-center px-2.5 hover:bg-white/5"
                      aria-label="Decrease CRF"
                      onclick={() => {
                        stepCrf(-1);
                      }}
                    >
                      <Minus class="h-3 w-3" />
                    </button>
                    <div
                      class="border-hairline flex items-center gap-2 border-x px-4"
                    >
                      <span class="font-mono text-xs font-medium"
                        >{encode.settings.crf}</span
                      >
                      {#if outputHeight != null}
                        <span class="text-base-content/50 text-[10px]"
                          >({outputHeight}p)</span
                        >
                      {/if}
                    </div>
                    <button
                      type="button"
                      class="flex h-8 items-center px-2.5 hover:bg-white/5"
                      aria-label="Increase CRF"
                      onclick={() => {
                        stepCrf(1);
                      }}
                    >
                      <Plus class="h-3 w-3" />
                    </button>
                  </div>
                {:else}
                  <label class="flex items-center gap-2">
                    <input
                      type="number"
                      min="100"
                      step="100"
                      class="input input-sm w-28 text-right font-mono"
                      bind:value={encode.settings.targetBitrateKbps}
                    />
                    <span class="text-base-content/50 text-[10px] uppercase"
                      >kbps</span
                    >
                  </label>
                {/if}
              </div>

              {#if encode.settings.name === "x265" || encode.settings.name === "x264"}
                <div class="flex items-center justify-between gap-3">
                  <label
                    class="flex items-center gap-3 {crfMode
                      ? 'opacity-50'
                      : ''}"
                  >
                    <span class="text-xs font-medium">Two-pass Encoding</span>
                    <input
                      type="checkbox"
                      class="toggle toggle-primary toggle-sm"
                      bind:checked={encode.settings.twoPass}
                      disabled={crfMode}
                    />
                  </label>
                  <span class="text-base-content/50 text-[10px]">
                    {crfMode
                      ? "Only applies in bitrate mode"
                      : "Requires longer encode time"}
                  </span>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </AccordionSection>

      <AccordionSection
        title="Audio"
        open={openSection === "audio"}
        onToggle={() => (openSection = "audio")}
      >
        {#if encode.audioTracks.length === 0}
          <p class="text-base-content/50 text-sm">
            {video.path
              ? "No audio tracks."
              : "Choose a source to see its audio tracks."}
          </p>
        {:else}
          <div class="flex flex-col">
            {#each encode.audioTracks as selection, i (selection.index)}
              {@const probed = video.info?.audioTracks[i]}
              {@const action = selection.action}
              <TrackRow
                label={`Track ${String(selection.index + 1)}`}
                language={probed?.language}
                title={probed?.title}
                isDefault={probed?.isDefault ?? false}
                enabled={selection.enabled}
                onToggle={(enabled: boolean) => (selection.enabled = enabled)}
              >
                <select
                  class="select select-xs"
                  value={action.action}
                  onchange={(event) => {
                    onAudioActionKindChange(selection, event);
                  }}
                >
                  <option value="copy">Copy</option>
                  <option value="convert">Convert</option>
                </select>
                {#if action.action === "convert"}
                  <select class="select select-xs" bind:value={action.codec}>
                    {#each config.availableAudioCodecs(action.codec) as codec (codec.id)}
                      <option value={codec.id}>{codec.label}</option>
                    {/each}
                  </select>
                  {#if !AUDIO_CODECS.find((c) => c.id === action.codec)?.lossless}
                    <input
                      type="range"
                      min="0"
                      max="9"
                      step="1"
                      class="range range-primary range-xs w-16 shrink-0"
                      bind:value={action.quality}
                      title={`Quality ${String(action.quality)} (0 best, 9 smallest)`}
                    />
                  {/if}
                  <select
                    class="select select-xs"
                    value={action.downmix === null
                      ? ""
                      : String(action.downmix)}
                    onchange={(event) => {
                      onDownmixChange(action, event);
                    }}
                  >
                    <option value="">Source channels</option>
                    {#if selection.sourceChannels > 2}
                      <option value="2">Stereo</option>
                    {/if}
                    {#if selection.sourceChannels > 1}
                      <option value="1">Mono</option>
                    {/if}
                  </select>
                {/if}
              </TrackRow>
            {/each}
          </div>
        {/if}
      </AccordionSection>

      <AccordionSection
        title="Subtitles"
        open={openSection === "subtitles"}
        onToggle={() => (openSection = "subtitles")}
      >
        {#if encode.subtitleTracks.length === 0}
          <p class="text-base-content/50 text-sm">
            {video.path
              ? "No subtitle tracks."
              : "Choose a source to see its subtitle tracks."}
          </p>
        {:else}
          <div class="flex flex-col">
            {#each encode.subtitleTracks as selection, i (selection.index)}
              {@const probed = video.info?.subtitleTracks[i]}
              <TrackRow
                label={`Track ${String(selection.index + 1)}${probed ? ` — ${probed.codecName.toUpperCase()}` : ""}`}
                language={probed?.language}
                title={probed?.title}
                isDefault={probed?.isDefault ?? false}
                enabled={selection.enabled}
                onToggle={(enabled: boolean) => (selection.enabled = enabled)}
              />
            {/each}
          </div>
          <p class="text-base-content/50 mt-3 text-xs">
            Subtitles are copied as-is — burn-in isn't available yet.
          </p>
        {/if}
      </AccordionSection>

      <AccordionSection
        title="Filters"
        open={openSection === "filters"}
        onToggle={() => (openSection = "filters")}
      >
        <div class="flex flex-col gap-5">
          <label class="flex items-center justify-between">
            <span class="text-sm">Deinterlace</span>
            <input
              type="checkbox"
              class="toggle toggle-primary toggle-sm"
              bind:checked={encode.filters.deinterlace}
            />
          </label>

          {#if encode.filters.crop}
            {@const crop = encode.filters.crop}
            <div class="flex flex-col gap-2">
              <span
                class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
                >Crop (pixels)</span
              >
              <div class="grid grid-cols-4 gap-2">
                <label class="form-control">
                  <span class="text-base-content/50 mb-1 text-xs">Top</span>
                  <input
                    type="number"
                    min="0"
                    class="input input-xs"
                    bind:value={crop.top}
                    onchange={commitCrop}
                  />
                </label>
                <label class="form-control">
                  <span class="text-base-content/50 mb-1 text-xs">Bottom</span>
                  <input
                    type="number"
                    min="0"
                    class="input input-xs"
                    bind:value={crop.bottom}
                    onchange={commitCrop}
                  />
                </label>
                <label class="form-control">
                  <span class="text-base-content/50 mb-1 text-xs">Left</span>
                  <input
                    type="number"
                    min="0"
                    class="input input-xs"
                    bind:value={crop.left}
                    onchange={commitCrop}
                  />
                </label>
                <label class="form-control">
                  <span class="text-base-content/50 mb-1 text-xs">Right</span>
                  <input
                    type="number"
                    min="0"
                    class="input input-xs"
                    bind:value={crop.right}
                    onchange={commitCrop}
                  />
                </label>
              </div>
              <p class="text-base-content/50 text-xs">
                Enabled via <span class="font-medium">Open Crop Tool</span> above
                — drag the frame's handles on the preview, or type exact pixel values
                here.
              </p>
            </div>
          {/if}

          <div class="border-hairline border-t pt-4">
            <p class="mb-2 text-sm font-medium">Trim</p>
            <div class="grid grid-cols-2 gap-3">
              <label class="form-control">
                <span class="text-base-content/50 mb-1 text-xs">Start (s)</span>
                <div class="join">
                  <input
                    type="number"
                    min="0"
                    step="0.1"
                    class="input input-xs join-item w-full"
                    placeholder="0"
                    value={encode.trim.startTime ?? ""}
                    oninput={(event) => {
                      onTrimInput("startTime", event);
                    }}
                  />
                  <button
                    type="button"
                    class="btn btn-xs join-item"
                    onclick={() => (encode.trim.startTime = video.scrubTime)}
                  >
                    Set
                  </button>
                </div>
              </label>
              <label class="form-control">
                <span class="text-base-content/50 mb-1 text-xs">End (s)</span>
                <div class="join">
                  <input
                    type="number"
                    min="0"
                    step="0.1"
                    class="input input-xs join-item w-full"
                    placeholder={video.info
                      ? video.info.durationSecs.toFixed(1)
                      : ""}
                    value={encode.trim.endTime ?? ""}
                    oninput={(event) => {
                      onTrimInput("endTime", event);
                    }}
                  />
                  <button
                    type="button"
                    class="btn btn-xs join-item"
                    onclick={() => (encode.trim.endTime = video.scrubTime)}
                  >
                    Set
                  </button>
                </div>
              </label>
            </div>
            <label class="mt-3 flex items-center justify-between">
              <span class="text-sm"
                >Fast seek
                <span class="text-base-content/50 text-xs">
                  (snaps to the nearest keyframe; faster, less exact)
                </span>
              </span>
              <input
                type="checkbox"
                class="toggle toggle-primary toggle-sm"
                bind:checked={encode.trim.fastSeek}
              />
            </label>
          </div>
        </div>
      </AccordionSection>

      <AccordionSection
        title="HDR / Advanced"
        summary={video.info?.hdr10 ? "HDR10 signaling on" : undefined}
        open={openSection === "hdr"}
        onToggle={() => (openSection = "hdr")}
      >
        {#snippet headerRight()}
          {#if video.info?.hdr10}
            <span
              class="border-warning/20 bg-warning/10 text-warning rounded border px-2 py-0.5 text-[10px] font-medium tracking-tight uppercase"
              >HDR10</span
            >
          {/if}
        {/snippet}
        <p class="text-base-content/50 text-sm">{HDR_NOTE}</p>
      </AccordionSection>

      <AccordionSection
        title="Command Preview"
        open={openSection === "command"}
        onToggle={() => (openSection = "command")}
      >
        {#if encode.commandPreview.length > 0}
          <div
            class="bg-base-100 border-hairline relative flex flex-col gap-3 rounded-lg border p-3"
          >
            <button
              type="button"
              class="btn btn-ghost btn-xs absolute top-2 right-2"
              onclick={copyCommand}
              aria-label="Copy command"
            >
              <Copy class="h-3.5 w-3.5" />
            </button>
            {#each encode.commandPreview as step, i (i)}
              {#if encode.commandPreview.length > 1}
                <span
                  class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
                >
                  Pass {i + 1}
                </span>
              {/if}
              <pre
                class="font-mono text-xs break-all whitespace-pre-wrap">{step.join(
                  " ",
                )}</pre>
            {/each}
          </div>
        {:else}
          <p class="text-base-content/50 text-sm">
            Choose a source to see the command that will run.
          </p>
        {/if}
      </AccordionSection>
    </div>
  </div>
</div>
