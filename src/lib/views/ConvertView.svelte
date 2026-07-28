<script lang="ts">
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import { video } from "$lib/stores/video.svelte";
  import AccordionSection from "$lib/components/AccordionSection.svelte";

  const sections = [
    {
      id: "quality",
      title: "Quality",
      note: "Encoder-specific settings land here once the encode pipeline (M3) is wired up.",
    },
    { id: "audio", title: "Audio", note: "Coming in M7." },
    { id: "subtitles", title: "Subtitles", note: "Coming in M7." },
    { id: "filters", title: "Filters", note: "Coming in M6." },
    { id: "hdr", title: "HDR / Advanced", note: "Coming in M8." },
    { id: "command", title: "Command Preview", note: "Coming in M3." },
  ] as const;

  let openSection: (typeof sections)[number]["id"] = $state("quality");

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
        disabled={!video.path}>Convert</button
      >
    </div>
  </header>

  {#if video.error}
    <div class="alert alert-error mx-6 mt-4 text-sm">{video.error}</div>
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
      {#each sections as section (section.id)}
        <AccordionSection
          title={section.title}
          open={openSection === section.id}
          onToggle={() => (openSection = section.id)}
        >
          <p class="text-base-content/50 text-sm">{section.note}</p>
        </AccordionSection>
      {/each}
    </div>
  </div>
</div>
