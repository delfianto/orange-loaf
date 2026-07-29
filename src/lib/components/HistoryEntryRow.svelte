<script lang="ts">
  import Play from "lucide-svelte/icons/play";
  import Trash2 from "lucide-svelte/icons/trash-2";
  import ChevronDown from "lucide-svelte/icons/chevron-down";
  import ChevronRight from "lucide-svelte/icons/chevron-right";
  import CheckCircle2 from "lucide-svelte/icons/check-circle-2";
  import XCircle from "lucide-svelte/icons/x-circle";
  import FileVideo from "lucide-svelte/icons/file-video";
  import { history, type HistoryEntry } from "$lib/stores/history.svelte";
  import {
    encode,
    ENCODER_NAMES,
    type EncoderSettings,
  } from "$lib/stores/encode.svelte";
  import { navigation } from "$lib/stores/navigation.svelte";

  interface Props {
    entry: HistoryEntry;
  }

  const { entry }: Props = $props();

  let expanded = $state(false);

  function sourceName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function encoderLabel(name: string): string {
    return ENCODER_NAMES.find((e) => e.id === name)?.label ?? name;
  }

  function settingsSummary(settings: EncoderSettings): string {
    switch (settings.name) {
      case "x265":
      case "x264":
        return `${settings.preset} · CRF ${String(settings.crf)}`;
      case "svtAv1":
        return `speed ${String(settings.speed)} · CRF ${String(settings.crf)}`;
      case "h264Videotoolbox":
      case "hevcVideotoolbox":
        return `quality ${String(settings.quality)}`;
    }
  }

  function formatTimestamp(unixMs: number): string {
    return new Date(unixMs).toLocaleString(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    });
  }

  // Only settings + filters transfer — see history.rs's `apply_history_entry`
  // doc comment for why trim/track selections deliberately don't.
  async function applyEntry(): Promise<void> {
    const applied = await history.apply(entry.id);
    encode.settings = applied.settings;
    encode.filters = applied.filters;
    navigation.current = "convert";
  }
</script>

<div>
  <!-- Same nested-button constraint as QueueJobRow: only the disclosure
       toggle is a <button>; the timestamp and action buttons are its
       siblings, not its children. -->
  <div class="group flex h-16 w-full items-center gap-3 px-4">
    <button
      type="button"
      class="flex min-w-0 flex-1 items-center gap-3 text-left"
      onclick={() => {
        expanded = !expanded;
      }}
    >
      {#if expanded}
        <ChevronDown class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
      {:else}
        <ChevronRight class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
      {/if}

      <div
        class="bg-base-100 border-hairline flex aspect-video h-11 shrink-0 items-center justify-center overflow-hidden rounded border"
      >
        {#if entry.thumbnail}
          <img
            src={entry.thumbnail}
            alt=""
            class="h-full w-full object-cover"
          />
        {:else}
          <FileVideo class="text-base-content/30 h-4 w-4" />
        {/if}
      </div>

      {#if entry.status.state === "completed"}
        <CheckCircle2 class="text-success h-4 w-4 shrink-0" />
      {:else}
        <XCircle class="text-error h-4 w-4 shrink-0" />
      {/if}

      <span class="flex min-w-0 flex-1 flex-col">
        <span class="truncate text-sm font-medium">
          {sourceName(entry.source)}
        </span>
        <span class="text-base-content/50 truncate text-[11px]">
          {settingsSummary(entry.settings)}
        </span>
      </span>

      <span class="badge badge-sm badge-outline shrink-0 font-mono uppercase">
        {encoderLabel(entry.settings.name)}
      </span>
    </button>

    <span class="flex shrink-0 items-center gap-3">
      <span class="text-base-content/50 font-mono text-[11px]">
        {formatTimestamp(entry.completedAtUnixMs)}
      </span>
    </span>

    <span
      class="flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100"
    >
      <button
        type="button"
        class="btn btn-ghost btn-xs"
        onclick={() => {
          void applyEntry();
        }}
      >
        <Play class="h-3.5 w-3.5" />
        Apply
      </button>
      <button
        type="button"
        class="btn btn-ghost btn-xs btn-square"
        onclick={() => {
          void history.remove(entry.id);
        }}
        aria-label="Delete"
      >
        <Trash2 class="h-3.5 w-3.5" />
      </button>
    </span>
  </div>

  {#if expanded}
    <div class="border-hairline space-y-3 border-t p-4">
      <div class="flex flex-col gap-1">
        <span
          class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
        >
          Source
        </span>
        <span class="font-mono text-xs break-all">{entry.source}</span>
      </div>
      <div class="flex flex-col gap-1">
        <span
          class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
        >
          Output
        </span>
        <span class="font-mono text-xs break-all">{entry.output}</span>
      </div>
      {#if entry.status.state === "failed"}
        <div class="flex flex-col gap-1">
          <span
            class="text-error/80 text-[10px] font-semibold tracking-wider uppercase"
          >
            Error
          </span>
          <p class="text-error font-mono text-xs whitespace-pre-wrap">
            {entry.status.message}
          </p>
        </div>
      {/if}
    </div>
  {/if}
</div>
