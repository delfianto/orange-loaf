<script lang="ts">
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import ChevronDown from "lucide-svelte/icons/chevron-down";
  import ChevronRight from "lucide-svelte/icons/chevron-right";
  import RefreshCw from "lucide-svelte/icons/refresh-cw";
  import CheckCircle2 from "lucide-svelte/icons/check-circle-2";
  import XCircle from "lucide-svelte/icons/x-circle";
  import Ban from "lucide-svelte/icons/ban";
  import CircleDashed from "lucide-svelte/icons/circle-dashed";
  import ArrowUp from "lucide-svelte/icons/arrow-up";
  import ArrowDown from "lucide-svelte/icons/arrow-down";
  import Trash2 from "lucide-svelte/icons/trash-2";
  import FolderSearch from "lucide-svelte/icons/folder-search";
  import Copy from "lucide-svelte/icons/copy";
  import { queue, type Job } from "$lib/stores/queue.svelte";
  import { ENCODER_NAMES } from "$lib/stores/encode.svelte";

  interface Props {
    job: Job;
    /** Position among its own section's jobs — only meaningful (and only
     * used) for queued jobs, to disable Move Up/Down at the ends. */
    index: number;
    total: number;
  }

  const { job, index, total }: Props = $props();

  let expanded = $state(false);

  function sourceName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function encoderLabel(name: string): string {
    return ENCODER_NAMES.find((e) => e.id === name)?.label ?? name;
  }

  function formatDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    if (h > 0) {
      return `${String(h)}h ${String(m)}m`;
    }
    if (m > 0) {
      return `${String(m)}m ${String(sec)}s`;
    }
    return `${String(sec)}s`;
  }

  // Derived from `speed` alone (output-seconds encoded per wall-clock
  // second) rather than tracking a separate wall-clock start timestamp —
  // `speed` already carries everything both figures need.
  const elapsedLabel = $derived.by(() => {
    const p = queue.progress[job.id];
    if (!p?.speed) {
      return null;
    }
    return formatDuration(p.outTimeSecs / p.speed);
  });

  const etaLabel = $derived.by(() => {
    const p = queue.progress[job.id];
    if (!p?.speed) {
      return null;
    }
    const remaining = Math.max(0, job.durationSecs - p.outTimeSecs);
    return formatDuration(remaining / p.speed);
  });

  function copyLog(): void {
    void navigator.clipboard.writeText((queue.logs[job.id] ?? []).join("\n"));
  }
</script>

<div>
  <!-- A `<button>` can't validly contain other buttons, so only the
       disclosure toggle (chevron/icon/name/badge) is one; the status
       display and action buttons are its siblings in this row, not its
       children. -->
  <div class="group flex h-11 w-full items-center gap-3 px-4">
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

      {#if job.status.state === "running"}
        <RefreshCw class="text-warning h-4 w-4 shrink-0 animate-spin" />
      {:else if job.status.state === "queued"}
        <CircleDashed class="text-base-content/50 h-4 w-4 shrink-0" />
      {:else if job.status.state === "completed"}
        <CheckCircle2 class="text-success h-4 w-4 shrink-0" />
      {:else if job.status.state === "cancelled"}
        <Ban class="text-base-content/50 h-4 w-4 shrink-0" />
      {:else}
        <XCircle class="text-error h-4 w-4 shrink-0" />
      {/if}

      <span class="max-w-md shrink truncate text-sm font-medium">
        {sourceName(job.source)}
      </span>
      <span class="badge badge-sm badge-outline shrink-0 font-mono uppercase">
        {encoderLabel(job.settings.name)}
      </span>
    </button>

    <span class="flex flex-1 items-center justify-end gap-3">
      {#if job.status.state === "running"}
        {@const percent = queue.progress[job.id]?.percent}
        <div class="bg-base-100 h-1 w-32 overflow-hidden rounded-full">
          <div
            class="bg-primary h-full"
            style:width="{String(percent ?? 0)}%"
          ></div>
        </div>
        <span
          class="text-base-content/50 w-24 shrink-0 text-right font-mono text-[11px]"
        >
          {etaLabel ? `ETA ${etaLabel}` : "starting…"}
        </span>
      {:else if job.status.state === "queued"}
        <span class="text-base-content/50 font-mono text-[11px]">
          Waiting…
        </span>
      {:else if job.status.state === "failed"}
        <span class="text-error/80 max-w-xs truncate text-[11px] italic">
          {job.status.message.split("\n")[0]}
        </span>
      {:else if job.status.state === "cancelled"}
        <span class="text-base-content/50 text-[11px] italic"> Cancelled </span>
      {/if}
    </span>

    <span
      class="flex shrink-0 items-center gap-1 opacity-0 group-hover:opacity-100"
    >
      {#if job.status.state === "queued"}
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          disabled={index === 0}
          onclick={() => {
            void queue.move(job.id, "up");
          }}
          aria-label="Move up"
        >
          <ArrowUp class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          disabled={index === total - 1}
          onclick={() => {
            void queue.move(job.id, "down");
          }}
          aria-label="Move down"
        >
          <ArrowDown class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          onclick={() => {
            void queue.remove(job.id);
          }}
          aria-label="Remove"
        >
          <Trash2 class="h-3.5 w-3.5" />
        </button>
      {:else if job.status.state === "running"}
        <button
          type="button"
          class="btn btn-ghost btn-xs"
          onclick={() => {
            void queue.cancel(job.id);
          }}
        >
          Cancel
        </button>
      {:else if job.status.state === "completed"}
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          onclick={() => {
            void revealItemInDir(job.output);
          }}
          aria-label="Open containing folder"
        >
          <FolderSearch class="h-3.5 w-3.5" />
        </button>
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          onclick={() => {
            void queue.remove(job.id);
          }}
          aria-label="Remove"
        >
          <Trash2 class="h-3.5 w-3.5" />
        </button>
      {:else}
        <button
          type="button"
          class="btn btn-ghost btn-xs"
          onclick={() => {
            void queue.retry(job);
          }}
        >
          Retry
        </button>
        <button
          type="button"
          class="btn btn-ghost btn-xs btn-square"
          onclick={() => {
            void queue.remove(job.id);
          }}
          aria-label="Remove"
        >
          <Trash2 class="h-3.5 w-3.5" />
        </button>
      {/if}
    </span>
  </div>

  {#if expanded}
    <div class="border-hairline space-y-3 border-t p-4">
      {#if job.status.state === "running" || job.status.state === "completed"}
        <div class="flex items-center gap-4">
          <div class="flex flex-col">
            <span
              class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >
              Time Left
            </span>
            <span class="font-mono text-xs">{etaLabel ?? "—"}</span>
          </div>
          <div class="bg-hairline h-6 w-px"></div>
          <div class="flex flex-col">
            <span
              class="text-base-content/50 text-[10px] font-semibold tracking-wider uppercase"
            >
              Elapsed
            </span>
            <span class="font-mono text-xs">{elapsedLabel ?? "—"}</span>
          </div>
        </div>
      {/if}
      {#if job.status.state === "failed"}
        <p class="text-error font-mono text-xs whitespace-pre-wrap">
          {job.status.message}
        </p>
      {/if}
      <div class="flex items-center justify-between">
        <span class="text-base-content/50 text-xs">Log</span>
        <button
          type="button"
          class="text-base-content/50 hover:text-base-content flex items-center gap-1.5 text-[10px] font-medium"
          onclick={copyLog}
        >
          <Copy class="h-3 w-3" />
          Copy Log
        </button>
      </div>
      <div
        class="bg-base-100 border-hairline max-h-40 overflow-y-auto rounded-lg border p-3 font-mono text-[12px] leading-relaxed"
      >
        {#if (queue.logs[job.id] ?? []).length === 0}
          <p class="text-base-content/40">No log output yet.</p>
        {:else}
          {#each queue.logs[job.id] ?? [] as line, lineIndex (lineIndex)}
            <div class="text-base-content/60">{line}</div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
