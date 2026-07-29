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
    // Optional right-aligned header content — status pills (10-bit/2-Pass on
    // Quality) or a capability badge (HDR10 on HDR/Advanced). Rendered inside
    // the header button, so it must be non-interactive content (spans), never
    // nested buttons.
    headerRight?: Snippet;
  }

  const { title, summary, open, onToggle, children, headerRight }: Props =
    $props();
</script>

<!-- No per-section border here: the parent panel supplies the hairline
     dividers via `gap-px` over a hairline background (see ConvertView's right
     column), matching the design mockup's single bordered card. -->
<div class="bg-base-300">
  <button
    type="button"
    class="flex w-full items-center justify-between px-5 py-4 text-left transition-colors hover:bg-white/5"
    onclick={onToggle}
    aria-expanded={open}
  >
    <span class="flex min-w-0 items-center gap-3">
      {#if open}
        <ChevronDown class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
      {:else}
        <ChevronRight class="text-base-content/50 h-3.5 w-3.5 shrink-0" />
      {/if}
      <span class="text-sm font-semibold">{title}</span>
      {#if summary && !open}
        <span class="text-base-content/50 truncate text-xs font-normal"
          >{summary}</span
        >
      {/if}
    </span>
    {#if headerRight}
      <span class="flex shrink-0 items-center gap-2"
        >{@render headerRight()}</span
      >
    {/if}
  </button>
  {#if open}
    <div class="px-5 pb-6">
      {@render children()}
    </div>
  {/if}
</div>
