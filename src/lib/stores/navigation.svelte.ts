export const VIEW_IDS = [
  "convert",
  "queue",
  "history",
  "profiles",
  "settings",
] as const;

export type ViewId = (typeof VIEW_IDS)[number];

class NavigationState {
  current: ViewId = $state("convert");
}

export const navigation = new NavigationState();
