<script lang="ts">
  import { onMount } from "svelte";
  import HistoryIcon from "lucide-svelte/icons/history";
  import { history } from "$lib/stores/history.svelte";
  import HistoryEntryRow from "$lib/components/HistoryEntryRow.svelte";

  onMount(() => {
    void history.refresh();
  });
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <div class="flex items-center gap-2">
      <h1 class="text-lg font-semibold">History</h1>
      <span class="badge badge-sm badge-outline">
        {history.entries.length}
        {history.entries.length === 1 ? "entry" : "entries"}
      </span>
    </div>
    <button
      type="button"
      class="btn btn-ghost btn-sm"
      disabled={history.entries.length === 0}
      onclick={() => {
        void history.clear();
      }}
    >
      Clear All
    </button>
  </header>

  {#if history.entries.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3">
      <HistoryIcon class="text-base-content/30 h-8 w-8" />
      <p class="text-base-content/50 text-sm">
        Finished jobs land here — encode something from Convert or the Queue to
        get started.
      </p>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto p-6">
      <div
        class="bg-base-300 border-hairline divide-hairline divide-y rounded-md border"
      >
        {#each history.entries as entry (entry.id)}
          <HistoryEntryRow {entry} />
        {/each}
      </div>
    </div>
  {/if}
</div>
