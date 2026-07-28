<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    language?: string | null;
    title?: string | null;
    isDefault?: boolean;
    enabled: boolean;
    onToggle: (enabled: boolean) => void;
    children?: Snippet;
  }

  const {
    label,
    language = null,
    title = null,
    isDefault = false,
    enabled,
    onToggle,
    children,
  }: Props = $props();
</script>

<div
  class="border-hairline flex items-center gap-3 border-b py-2 last:border-b-0"
>
  <input
    type="checkbox"
    class="toggle toggle-primary toggle-sm shrink-0"
    checked={enabled}
    onchange={(event) => {
      onToggle(event.currentTarget.checked);
    }}
  />
  <div class="min-w-0 flex-1">
    <div class="flex items-center gap-2">
      <span class="text-sm font-medium">{label}</span>
      {#if language}
        <span class="badge badge-sm badge-outline uppercase">{language}</span>
      {/if}
      {#if isDefault}
        <span class="badge badge-sm badge-outline">Default</span>
      {/if}
    </div>
    {#if title}
      <p class="text-base-content/50 truncate text-xs">{title}</p>
    {/if}
  </div>
  {#if children}
    <div class="flex shrink-0 items-center gap-2">
      {@render children()}
    </div>
  {/if}
</div>
