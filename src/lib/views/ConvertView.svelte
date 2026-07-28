<script lang="ts">
  import FolderOpen from "lucide-svelte/icons/folder-open";
  import { source } from "$lib/stores/source.svelte";
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
</script>

<div class="flex h-full flex-col">
  <header
    class="border-hairline flex h-14 shrink-0 items-center justify-between border-b px-6"
  >
    <div class="min-w-0">
      {#if source.path}
        <p class="truncate text-sm font-medium">{sourceName(source.path)}</p>
        <p class="text-base-content/50 truncate text-xs">{source.path}</p>
      {:else}
        <p class="text-base-content/50 text-sm">No source selected</p>
      {/if}
    </div>
    <div class="flex items-center gap-3">
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        onclick={() => source.choose()}
      >
        <FolderOpen class="h-4 w-4" />
        {source.path ? "Change Source" : "Choose Source"}
      </button>
      <button
        type="button"
        class="btn btn-ghost btn-sm"
        disabled={!source.path}
      >
        Add to Queue
      </button>
      <button
        type="button"
        class="btn btn-primary btn-sm"
        disabled={!source.path}
      >
        Convert
      </button>
    </div>
  </header>

  <div class="grid flex-1 grid-cols-[3fr_2fr] gap-6 overflow-y-auto p-6">
    <div
      class="bg-base-300 border-hairline flex aspect-video min-w-0 items-center justify-center rounded-lg border"
    >
      <p class="text-base-content/50 text-sm">
        {source.path
          ? "Preview not available yet"
          : "Choose a source to get started"}
      </p>
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
