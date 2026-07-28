<script lang="ts">
  import type { Snippet } from "svelte";
  import ChevronDown from "lucide-svelte/icons/chevron-down";
  import ChevronRight from "lucide-svelte/icons/chevron-right";

  interface Props {
    title: string;
    summary?: string;
    open: boolean;
    onToggle: () => void;
    children: Snippet;
  }

  const { title, summary, open, onToggle, children }: Props = $props();
</script>

<div class="border-hairline border-b">
  <button
    type="button"
    class="flex h-11 w-full items-center gap-2 text-left"
    onclick={onToggle}
    aria-expanded={open}
  >
    {#if open}
      <ChevronDown class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
    {:else}
      <ChevronRight class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
    {/if}
    <span class="font-medium">{title}</span>
    {#if summary && !open}
      <span class="text-base-content/50 truncate text-sm">{summary}</span>
    {/if}
  </button>
  {#if open}
    <div class="pb-4">
      {@render children()}
    </div>
  {/if}
</div>
