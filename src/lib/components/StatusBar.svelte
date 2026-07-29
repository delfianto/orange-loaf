<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import Cpu from "lucide-svelte/icons/cpu";
  import HardDrive from "lucide-svelte/icons/hard-drive";
  import { tools, type ToolStatus } from "$lib/stores/tools.svelte";
  import { queue } from "$lib/stores/queue.svelte";

  let appVersion = $state("");

  onMount(() => {
    void getVersion().then((version) => (appVersion = version));
  });

  const ffmpeg = $derived(tools.statuses.find((s) => s.name === "FFmpeg"));
  const ffmpegLabel = $derived(formatFfmpegLabel(ffmpeg));
  const pendingCount = $derived(
    queue.jobs.filter(
      (job) => job.status.state === "queued" || job.status.state === "running",
    ).length,
  );

  // Settings shows the full `ffmpeg -version` banner ("ffmpeg version 8.1.2
  // Copyright (c) 2000-2026 the FFmpeg developers"); this strip only has
  // room for the version token itself.
  function formatFfmpegLabel(status: ToolStatus | undefined): string {
    if (!status) {
      return "FFmpeg …";
    }
    if (!status.found) {
      return "FFmpeg not found";
    }
    const version = status.version?.match(/version\s+(\S+)/i)?.[1];
    return version ? `FFmpeg ${version}` : "FFmpeg";
  }
</script>

<footer
  class="border-hairline bg-base-200 text-base-content/50 flex h-8 shrink-0 items-center justify-between border-t px-4 text-[10px] font-semibold tracking-wider uppercase"
>
  <div class="flex items-center gap-6">
    <span class="flex items-center gap-2">
      <Cpu class="h-3 w-3" />
      {ffmpegLabel}
    </span>
    <span class="flex items-center gap-2">
      <HardDrive class="h-3 w-3 {tools.nvencAvailable ? 'text-success' : ''}" />
      {#if tools.nvencAvailable === null}
        Checking NVENC…
      {:else if tools.nvencAvailable}
        NVENC Available
      {:else}
        NVENC Unavailable
      {/if}
    </span>
  </div>
  <div class="flex items-center gap-4">
    <span>Queue: {pendingCount} Pending</span>
    <span>Build {appVersion}</span>
  </div>
</footer>
