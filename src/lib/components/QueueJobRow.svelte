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
  import GripVertical from "lucide-svelte/icons/grip-vertical";
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
  // Checked by default, matching design/mockups/queue-screen.html — libx264/
  // libx265's verbose stderr includes real per-frame NAL-unit trace lines
  // that are rarely what someone wants to read while skimming for an error.
  // `\bnal\b` (word-boundary, case-insensitive) rather than a plain
  // substring match — a naive `.includes("nal")` would also hide any line
  // containing "final"/"original"/etc.
  // `checked`/`onchange` rather than `bind:checked`: a bare local `let`
  // bound with `bind:checked` isn't visible to typescript-eslint's
  // control-flow analysis as ever being reassigned, so it narrows this to
  // the literal `true` and flags every read below as "always truthy" — an
  // explicit reassignment site is enough to keep the type as `boolean`.
  let hideNal: boolean = $state(true);
  const NAL_LINE_PATTERN = /\bnal\b/i;

  const visibleLog = $derived.by(() => {
    const lines = queue.logs[job.id] ?? [];
    return hideNal
      ? lines.filter((line) => !NAL_LINE_PATTERN.test(line))
      : lines;
  });

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

  function formatBytes(bytes: number): string {
    const units = ["B", "KB", "MB", "GB", "TB"];
    let value = bytes;
    let unitIndex = 0;
    while (value >= 1024 && unitIndex < units.length - 1) {
      value /= 1024;
      unitIndex += 1;
    }
    return `${value.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex] ?? "B"}`;
  }

  // Fixed-width H:MM:SS rather than `formatDuration`'s "4m 12s" — a
  // completed encode's elapsed time can run into hours, where a clock
  // format reads better than the ETA's more conversational style.
  function formatClockDuration(totalSeconds: number): string {
    const s = Math.max(0, Math.round(totalSeconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    const sec = s % 60;
    const pad = (n: number) => String(n).padStart(2, "0");
    return `${pad(h)}:${pad(m)}:${pad(sec)}`;
  }

  const completedSizeLabel = $derived(
    job.outputSizeBytes != null ? formatBytes(job.outputSizeBytes) : null,
  );
  const completedDurationLabel = $derived(
    job.startedAtUnixMs != null && job.finishedAtUnixMs != null
      ? formatClockDuration((job.finishedAtUnixMs - job.startedAtUnixMs) / 1000)
      : null,
  );

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
    void navigator.clipboard.writeText(visibleLog.join("\n"));
  }

  // Per-line log colouring, matching the mockup: warnings amber, errors red,
  // and the periodic `frame=…` progress summary brightened above the muted
  // per-frame trace. Whole-word matches so a path containing "error" in a
  // normal trace line doesn't light up the whole log.
  function logLineClass(line: string): string {
    if (/\berror\b/i.test(line)) {
      return "text-error";
    }
    if (/\bwarning\b/i.test(line)) {
      return "text-warning";
    }
    if (line.startsWith("frame=")) {
      return "text-base-content/80";
    }
    return "text-base-content/60";
  }

  // Only queued rows are draggable (matching the existing Move Up/Down
  // buttons' own precedent — reordering a running/finished job isn't
  // meaningful), so `dropPosition` only ever matters while both the
  // dragged and hovered row are queued.
  let dropPosition: "before" | "after" | null = $state(null);

  function onDragStart(event: DragEvent): void {
    event.dataTransfer?.setData("text/plain", job.id);
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
    }
  }

  function onDragOver(event: DragEvent): void {
    event.preventDefault();
    const row = event.currentTarget as HTMLElement;
    const rect = row.getBoundingClientRect();
    dropPosition =
      event.clientY > rect.top + rect.height / 2 ? "after" : "before";
  }

  function onDragLeave(): void {
    dropPosition = null;
  }

  function onDrop(event: DragEvent): void {
    event.preventDefault();
    const draggedId = event.dataTransfer?.getData("text/plain");
    const position = dropPosition;
    dropPosition = null;
    if (!draggedId || draggedId === job.id) {
      return;
    }
    void queue.reorder(draggedId, position === "after" ? index + 1 : index);
  }
</script>

<div>
  <!-- A `<button>` can't validly contain other buttons, so only the
       disclosure toggle (chevron/icon/name/badge) is one; the status
       display and action buttons are its siblings in this row, not its
       children. The drag handle is a sibling for the same reason, plus a
       practical one: nesting a `draggable` element inside the disclosure
       button would make a drag gesture also fire its click. -->
  <div
    class="group flex h-11 w-full items-center gap-3 border-t-2 border-b-2 px-4 {dropPosition ===
    'before'
      ? 'border-t-primary'
      : 'border-t-transparent'} {dropPosition === 'after'
      ? 'border-b-primary'
      : 'border-b-transparent'}"
    ondragover={job.status.state === "queued" ? onDragOver : undefined}
    ondragleave={job.status.state === "queued" ? onDragLeave : undefined}
    ondrop={job.status.state === "queued" ? onDrop : undefined}
  >
    {#if job.status.state === "queued"}
      <GripVertical
        class="text-base-content/40 hover:text-base-content/70 h-3.5 w-3.5 shrink-0 cursor-grab"
        draggable="true"
        ondragstart={onDragStart}
      />
    {/if}
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

      <span
        class="max-w-md shrink truncate text-sm font-medium {job.status
          .state === 'completed'
          ? 'text-base-content/50'
          : ''}"
      >
        {sourceName(job.source)}
      </span>
      <span class="flex shrink-0 items-center gap-1.5">
        <span
          class="border-hairline text-base-content/70 rounded border bg-white/5 px-1.5 py-0.5 font-mono text-[9px] uppercase"
        >
          {encoderLabel(job.settings.name)}
        </span>
        {#if job.videoInfo?.hdr10}
          <span
            class="border-hairline text-base-content/70 rounded border bg-white/5 px-1.5 py-0.5 font-mono text-[9px] uppercase"
          >
            HDR10
          </span>
        {/if}
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
      {:else if job.status.state === "completed"}
        {#if completedSizeLabel}
          <span class="text-base-content/50 font-mono text-[11px]"
            >{completedSizeLabel}</span
          >
        {/if}
        {#if completedDurationLabel}
          <span class="text-base-content/50 font-mono text-[11px]"
            >{completedDurationLabel}</span
          >
        {/if}
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
          class="btn btn-ghost btn-xs"
          onclick={() => {
            void revealItemInDir(job.output);
          }}
        >
          <FolderSearch class="h-3.5 w-3.5" />
          Open Folder
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
        <div class="flex items-center gap-3">
          <label
            class="text-base-content/60 flex items-center gap-1.5 text-[11px]"
          >
            <input
              type="checkbox"
              class="checkbox checkbox-xs"
              checked={hideNal}
              onchange={(
                event: Event & { currentTarget: HTMLInputElement },
              ) => {
                hideNal = event.currentTarget.checked;
              }}
            />
            Hide NAL unit messages
          </label>
          <button
            type="button"
            class="text-base-content/50 hover:text-base-content flex items-center gap-1.5 text-[10px] font-medium"
            onclick={copyLog}
          >
            <Copy class="h-3 w-3" />
            Copy Log
          </button>
        </div>
      </div>
      <div
        class="bg-base-100 border-hairline max-h-40 overflow-y-auto rounded-lg border p-3 font-mono text-[12px] leading-relaxed"
      >
        {#if visibleLog.length === 0}
          <p class="text-base-content/40">
            {(queue.logs[job.id] ?? []).length === 0
              ? "No log output yet."
              : "All log lines are hidden by the NAL filter above."}
          </p>
        {:else}
          {#each visibleLog as line, lineIndex (lineIndex)}
            <div class={logLineClass(line)}>{line}</div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>
