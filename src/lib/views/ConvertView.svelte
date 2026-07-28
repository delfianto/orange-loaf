<script lang="ts">
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import Copy from "lucide-svelte/icons/copy";
  import { video } from "$lib/stores/video.svelte";
  import {
    encode,
    ENCODER_NAMES,
    X265_PRESETS,
    X264_PRESETS,
    type EncoderName,
  } from "$lib/stores/encode.svelte";
  import AccordionSection from "$lib/components/AccordionSection.svelte";

  type SectionId =
    "quality" | "audio" | "subtitles" | "filters" | "hdr" | "command";
  let openSection: SectionId = $state("quality");

  const stubSections: { id: SectionId; title: string; note: string }[] = [
    { id: "audio", title: "Audio", note: "Coming in M7." },
    { id: "subtitles", title: "Subtitles", note: "Coming in M7." },
    { id: "filters", title: "Filters", note: "Coming in M6." },
    { id: "hdr", title: "HDR / Advanced", note: "Coming in M8." },
  ];

  $effect(() => {
    const path = video.path;
    const settings = encode.settings;
    const filters = encode.filters;
    void encode.refreshPreview(path, settings, filters);
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

  function onConvertClick(): void {
    if (!video.path || !video.info || encode.status?.state === "running") {
      return;
    }
    void encode.start(video.path, video.info.durationSecs);
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
      <button type="button" class="btn btn-ghost btn-sm" disabled={!video.path}
        >Add to Queue</button
      >
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
          {/if}
        </div>
      </AccordionSection>

      {#each stubSections as section (section.id)}
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
