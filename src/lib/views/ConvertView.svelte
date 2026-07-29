<script lang="ts">
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import Copy from "lucide-svelte/icons/copy";
  import RotateCw from "lucide-svelte/icons/rotate-cw";
  import FlipHorizontal from "lucide-svelte/icons/flip-horizontal-2";
  import FlipVertical from "lucide-svelte/icons/flip-vertical-2";
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
  import AccordionSection from "$lib/components/AccordionSection.svelte";
  import CropOverlay from "$lib/components/CropOverlay.svelte";
  import TrackRow from "$lib/components/TrackRow.svelte";

  type SectionId =
    "quality" | "audio" | "subtitles" | "filters" | "hdr" | "command";
  let openSection: SectionId = $state("quality");

  // "Filters" and "Audio"/"Subtitles" are now real; only HDR remains a stub.
  const postFilterStubs: { id: SectionId; title: string; note: string }[] = [
    {
      id: "hdr",
      title: "HDR / Advanced",
      note: "HDR10 mastering-display/light-level metadata is preserved automatically for HEVC (x265) and AV1 (SVT-AV1) outputs — there's nothing to configure here yet. HDR10+, tonemap, and the other advanced controls are still coming.",
    },
  ];

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

  function onToggleCrop(
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    if (!event.currentTarget.checked) {
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

  function onScaleInput(
    dimension: "scaleWidth" | "scaleHeight",
    event: Event & { currentTarget: HTMLInputElement },
  ): void {
    const raw = event.currentTarget.value;
    encode.filters[dimension] =
      raw === "" ? null : Math.max(1, Math.round(Number(raw)));
  }

  function rotateClockwise(): void {
    encode.filters.rotateDegrees = (encode.filters.rotateDegrees + 90) % 360;
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
    void navigator.clipboard.writeText(encode.commandPreview.join(" "));
  }

  const isRunning = $derived(encode.status?.state === "running");
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <div class="min-w-0">
      {#if video.path}
        <div class="flex items-center gap-2">
          <p class="truncate text-sm font-medium">{sourceName(video.path)}</p>
          {#if video.info}
            <span class="badge badge-sm badge-outline font-mono"
              >{video.info.width}×{video.info.height}</span
            >
            <span class="badge badge-sm badge-outline font-mono uppercase"
              >{video.info.codecName}</span
            >
            <span class="badge badge-sm badge-outline"
              >{video.info.bitDepth}-bit</span
            >
            {#if video.info.hdr10}
              <span class="badge badge-sm badge-warning badge-outline"
                >HDR10</span
              >
            {/if}
          {/if}
        </div>
        <p class="text-base-content/50 truncate text-xs">{video.path}</p>
      {:else}
        <p class="text-base-content/50 text-sm">No source selected</p>
      {/if}
    </div>
    <div class="flex items-center gap-3">
      <select
        class="select select-sm"
        value={encode.settings.name}
        onchange={onEncoderChange}
      >
        {#each ENCODER_NAMES as enc (enc.id)}
          <option value={enc.id}>{enc.label}</option>
        {/each}
      </select>
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        onclick={() => video.choose()}
      >
        <FolderOpen class="h-4 w-4" />
        {video.path ? "Change Source" : "Choose Source"}
      </button>
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

  <div class="grid flex-1 grid-cols-[3fr_2fr] gap-6 overflow-y-auto p-6">
    <div class="flex min-w-0 flex-col gap-2">
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
    </div>

    <div class="min-w-0">
      <AccordionSection
        title="Quality"
        open={openSection === "quality"}
        onToggle={() => (openSection = "quality")}
      >
        <div class="grid grid-cols-2 gap-x-4 gap-y-3">
          {#if encode.settings.name === "x265"}
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Preset</span>
              <select
                class="select select-sm"
                bind:value={encode.settings.preset}
              >
                {#each X265_PRESETS as preset (preset)}
                  <option value={preset}>{preset}</option>
                {/each}
              </select>
            </label>
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Tune</span>
              <select
                class="select select-sm"
                value={encode.settings.tune ?? ""}
                onchange={onTuneChange}
              >
                <option value="">Default</option>
                <option value="grain">Grain</option>
                <option value="animation">Animation</option>
                <option value="fastdecode">Fast Decode</option>
                <option value="zerolatency">Zero Latency</option>
              </select>
            </label>
            <label class="form-control col-span-2">
              <span class="text-base-content/50 mb-1 text-xs"
                >CRF <span class="font-mono">{encode.settings.crf}</span></span
              >
              <input
                type="range"
                min="0"
                max="51"
                step="1"
                bind:value={encode.settings.crf}
                class="range range-primary range-xs"
              />
            </label>
          {:else if encode.settings.name === "x264"}
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Preset</span>
              <select
                class="select select-sm"
                bind:value={encode.settings.preset}
              >
                {#each X264_PRESETS as preset (preset)}
                  <option value={preset}>{preset}</option>
                {/each}
              </select>
            </label>
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Tune</span>
              <select
                class="select select-sm"
                value={encode.settings.tune ?? ""}
                onchange={onTuneChange}
              >
                <option value="">Default</option>
                <option value="film">Film</option>
                <option value="animation">Animation</option>
                <option value="grain">Grain</option>
                <option value="fastdecode">Fast Decode</option>
                <option value="zerolatency">Zero Latency</option>
              </select>
            </label>
            <label class="form-control col-span-2">
              <span class="text-base-content/50 mb-1 text-xs"
                >CRF <span class="font-mono">{encode.settings.crf}</span></span
              >
              <input
                type="range"
                min="0"
                max="51"
                step="1"
                bind:value={encode.settings.crf}
                class="range range-primary range-xs"
              />
            </label>
          {:else if encode.settings.name === "svtAv1"}
            <label class="form-control col-span-2">
              <span class="text-base-content/50 mb-1 text-xs"
                >Preset (speed) <span class="font-mono"
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
            <label class="form-control col-span-2">
              <span class="text-base-content/50 mb-1 text-xs"
                >CRF <span class="font-mono">{encode.settings.crf}</span></span
              >
              <input
                type="range"
                min="0"
                max="63"
                step="1"
                bind:value={encode.settings.crf}
                class="range range-primary range-xs"
              />
            </label>
          {:else if encode.settings.name === "h264Videotoolbox" || encode.settings.name === "hevcVideotoolbox"}
            <label class="form-control col-span-2">
              <span class="text-base-content/50 mb-1 text-xs"
                >Quality <span class="font-mono">{encode.settings.quality}</span
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

          <div class="flex flex-col gap-2">
            <label class="flex items-center justify-between">
              <span class="text-sm">Crop</span>
              <input
                type="checkbox"
                class="toggle toggle-primary toggle-sm"
                checked={encode.filters.crop != null}
                disabled={!video.info}
                onchange={onToggleCrop}
              />
            </label>
            {#if encode.filters.crop}
              {@const crop = encode.filters.crop}
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
                Drag the frame's handles above to crop visually, or type exact
                pixel values here.
              </p>
            {/if}
          </div>

          <div class="grid grid-cols-2 gap-2">
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Scale width</span>
              <input
                type="number"
                min="1"
                placeholder="auto"
                class="input input-xs"
                value={encode.filters.scaleWidth ?? ""}
                oninput={(event) => {
                  onScaleInput("scaleWidth", event);
                }}
              />
            </label>
            <label class="form-control">
              <span class="text-base-content/50 mb-1 text-xs">Scale height</span
              >
              <input
                type="number"
                min="1"
                placeholder="auto"
                class="input input-xs"
                value={encode.filters.scaleHeight ?? ""}
                oninput={(event) => {
                  onScaleInput("scaleHeight", event);
                }}
              />
            </label>
          </div>

          <div class="flex items-center gap-2">
            <button
              type="button"
              class="btn btn-sm"
              onclick={rotateClockwise}
              aria-label="Rotate 90° clockwise"
            >
              <RotateCw class="h-4 w-4" />
              {encode.filters.rotateDegrees}°
            </button>
            <button
              type="button"
              class="btn btn-sm"
              class:btn-primary={encode.filters.horizontalFlip}
              onclick={() => {
                encode.filters.horizontalFlip = !encode.filters.horizontalFlip;
              }}
              aria-label="Flip horizontally"
            >
              <FlipHorizontal class="h-4 w-4" />
            </button>
            <button
              type="button"
              class="btn btn-sm"
              class:btn-primary={encode.filters.verticalFlip}
              onclick={() => {
                encode.filters.verticalFlip = !encode.filters.verticalFlip;
              }}
              aria-label="Flip vertically"
            >
              <FlipVertical class="h-4 w-4" />
            </button>
          </div>

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

      {#each postFilterStubs as section (section.id)}
        <AccordionSection
          title={section.title}
          open={openSection === section.id}
          onToggle={() => (openSection = section.id)}
        >
          <p class="text-base-content/50 text-sm">{section.note}</p>
        </AccordionSection>
      {/each}

      <AccordionSection
        title="Command Preview"
        open={openSection === "command"}
        onToggle={() => (openSection = "command")}
      >
        {#if encode.commandPreview.length > 0}
          <div
            class="bg-base-100 border-hairline relative rounded-lg border p-3"
          >
            <button
              type="button"
              class="btn btn-ghost btn-xs absolute top-2 right-2"
              onclick={copyCommand}
              aria-label="Copy command"
            >
              <Copy class="h-3.5 w-3.5" />
            </button>
            <pre
              class="font-mono text-xs break-all whitespace-pre-wrap">{encode.commandPreview.join(
                " ",
              )}</pre>
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
