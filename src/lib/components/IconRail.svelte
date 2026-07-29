<script lang="ts">
  import type { Component } from "svelte";
  import Zap from "lucide-svelte/icons/zap";
  import RefreshCw from "lucide-svelte/icons/refresh-cw";
  import ListOrdered from "lucide-svelte/icons/list-ordered";
  import HistoryIcon from "lucide-svelte/icons/history";
  import Layers from "lucide-svelte/icons/layers";
  import SettingsIcon from "lucide-svelte/icons/settings";
  import { navigation, type ViewId } from "$lib/stores/navigation.svelte";
  import { queue } from "$lib/stores/queue.svelte";
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

  // Mirrors StatusBar.svelte's own "Queue: N Pending" count — kept as a
  // separate local `$derived` rather than a shared store getter, matching
  // how every view already computes its own filters over `queue.jobs`
  // rather than centralizing them.
  const pendingCount = $derived(
    queue.jobs.filter(
      (job) => job.status.state === "queued" || job.status.state === "running",
    ).length,
  );
</script>

<nav
  class="bg-base-200 border-hairline flex h-full w-18 flex-col items-center border-r py-3"
>
  <div
    class="bg-primary mb-6 flex h-9 w-9 items-center justify-center rounded-lg"
  >
    <Zap class="text-primary-content h-5 w-5" />
  </div>

  <ul class="flex w-full flex-1 flex-col items-center gap-2">
    {#each navItems as item (item.id)}
      <li class="tooltip tooltip-right" data-tip={item.label}>
        <button
          type="button"
          class="relative flex h-10 w-10 items-center justify-center rounded-lg transition-colors duration-150 {navigation.current ===
          item.id
            ? 'bg-primary/15 text-primary'
            : 'text-base-content/70 hover:bg-base-300 hover:text-base-content'}"
          onclick={() => (navigation.current = item.id)}
          aria-current={navigation.current === item.id}
          aria-label={item.label}
        >
          <item.icon class="h-5 w-5" />
          {#if item.id === "queue" && pendingCount > 0}
            <span
              class="bg-warning text-warning-content absolute top-0.5 right-0.5 flex h-4 w-4 items-center justify-center rounded-full text-[10px] font-bold"
            >
              {pendingCount}
            </span>
          {/if}
        </button>
      </li>
    {/each}

    <!-- Settings is now an ordinary nav item pinned to the bottom (mt-auto),
         inside the same list as the others. It used to be a <div> sibling
         placed *after* this flex-1 <ul> — the one nav item built differently.
         A user reported its click doing nothing in the desktop WebView (works
         in Chromium); unifying the structure so it's identical to the working
         items is the leading hypothesis for that WebKit-only difference, but
         it's unverified against the desktop WebView from here. -->
    <li class="tooltip tooltip-right mt-auto" data-tip={m.nav_settings()}>
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
    </li>
  </ul>
</nav>
