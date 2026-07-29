<script lang="ts">
  import type { Component } from "svelte";
  import Zap from "lucide-svelte/icons/zap";
  import RefreshCw from "lucide-svelte/icons/refresh-cw";
  import ListOrdered from "lucide-svelte/icons/list-ordered";
  import HistoryIcon from "lucide-svelte/icons/history";
  import Layers from "lucide-svelte/icons/layers";
  import SettingsIcon from "lucide-svelte/icons/settings";
  import { navigation, type ViewId } from "$lib/stores/navigation.svelte";
  import { m } from "$lib/i18n.svelte";

  interface NavItem {
    id: ViewId;
    label: string;
    icon: Component;
  }

  const navItems: NavItem[] = [
    { id: "convert", label: m.nav_convert(), icon: RefreshCw },
    { id: "queue", label: m.nav_queue(), icon: ListOrdered },
    { id: "history", label: m.nav_history(), icon: HistoryIcon },
    { id: "profiles", label: m.nav_profiles(), icon: Layers },
  ];
</script>

<nav
  class="bg-base-200 border-hairline flex h-full w-18 flex-col items-center border-r py-3"
>
  <div
    class="bg-primary mb-6 flex h-9 w-9 items-center justify-center rounded-lg"
  >
    <Zap class="text-primary-content h-5 w-5" />
  </div>

  <ul class="flex flex-1 flex-col gap-2">
    {#each navItems as item (item.id)}
      <li class="tooltip tooltip-right" data-tip={item.label}>
        <button
          type="button"
          class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors duration-150 {navigation.current ===
          item.id
            ? 'bg-primary/15 text-primary'
            : 'text-base-content/70 hover:bg-base-300 hover:text-base-content'}"
          onclick={() => (navigation.current = item.id)}
          aria-current={navigation.current === item.id}
          aria-label={item.label}
        >
          <item.icon class="h-5 w-5" />
        </button>
      </li>
    {/each}
  </ul>

  <div class="tooltip tooltip-right" data-tip={m.nav_settings()}>
    <button
      type="button"
      class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors duration-150 {navigation.current ===
      'settings'
        ? 'bg-primary/15 text-primary'
        : 'text-base-content/70 hover:bg-base-300 hover:text-base-content'}"
      onclick={() => (navigation.current = "settings")}
      aria-current={navigation.current === "settings"}
      aria-label={m.nav_settings()}
    >
      <SettingsIcon class="h-5 w-5" />
    </button>
  </div>
</nav>
