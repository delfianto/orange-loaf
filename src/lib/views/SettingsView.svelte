<script lang="ts">
  import CircleCheck from "lucide-svelte/icons/circle-check";
  import CircleX from "lucide-svelte/icons/circle-x";
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import Loader from "lucide-svelte/icons/loader-circle";
  import { tools } from "$lib/stores/tools.svelte";

  const knownTools = [
    { key: "ffmpeg" as const, displayName: "FFmpeg" },
    { key: "ffprobe" as const, displayName: "FFprobe" },
  ];

  $effect(() => {
    void tools.refresh();
  });
</script>

<div class="mx-auto max-w-2xl p-6">
  <h1 class="mb-1 text-lg font-medium">Settings</h1>
  <p class="text-base-content/50 mb-6 text-sm">
    More sections (naming, profiles, language, ...) land as their milestones do
    — see notes/plan.md.
  </p>

  <section>
    <h2 class="text-base-content/70 mb-3 text-sm font-medium">
      Detected External Programs
    </h2>
    <div class="border-hairline divide-hairline divide-y rounded-lg border">
      {#each knownTools as known (known.key)}
        {@const status = tools.statuses.find(
          (s) => s.name === known.displayName,
        )}
        <div class="flex items-center gap-3 px-4 py-3">
          {#if tools.loading && !status}
            <Loader
              class="text-base-content/40 h-4 w-4 shrink-0 animate-spin"
            />
          {:else if status?.found}
            <CircleCheck class="text-success h-4 w-4 shrink-0" />
          {:else}
            <CircleX class="text-error h-4 w-4 shrink-0" />
          {/if}

          <div class="min-w-0 flex-1">
            <p class="text-sm font-medium">{known.displayName}</p>
            {#if status?.found}
              <p class="text-base-content/50 truncate font-mono text-xs">
                {status.path}
              </p>
              {#if status.version}
                <p class="text-base-content/40 truncate text-xs">
                  {status.version}
                </p>
              {/if}
            {:else}
              <p class="text-base-content/50 text-xs">Not found on PATH</p>
            {/if}
          </div>

          <button
            type="button"
            class="btn btn-ghost btn-xs"
            onclick={() => tools.browseFor(known.key)}
          >
            <FolderOpen class="h-3.5 w-3.5" />
            Browse
          </button>
        </div>
      {/each}
    </div>
  </section>
</div>
