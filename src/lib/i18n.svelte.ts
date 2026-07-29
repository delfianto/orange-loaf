import {
  getLocale,
  locales,
  setLocale as paraglideSetLocale,
  type Locale,
} from "../paraglide/runtime";

export type { Locale };
export { locales };
// Re-exported so every component that needs translated strings can import
// both `m` and `i18n`/`isLocale` from this one module.
export { m } from "../paraglide/messages";

// paraglide's message functions (`m.xxx()`) call its own `getLocale()`
// internally at call time — they are plain functions, not Svelte-reactive.
// `setLocale(..., { reload: false })` (the "globalVariable" strategy's
// client-only escape hatch) updates that internal value but, by its own
// documented contract, "does not re-render the UI or update the document."
// This class is what actually makes a locale switch visible without a
// reload: components read `i18n.locale` (a real rune) inside a `{#key}`
// block wrapping translated content, so Svelte remounts (and therefore
// re-evaluates every `m.xxx()` call in) that subtree when it changes.
class I18nState {
  locale: Locale = $state(getLocale());

  setLocale(locale: Locale): void {
    // Always synchronous in practice — the "globalVariable"/"baseLocale"
    // strategy has no custom async handler — but the type is generic
    // (`void | Promise<void>`) since paraglide allows async custom
    // strategies elsewhere.
    void paraglideSetLocale(locale, { reload: false });
    this.locale = locale;
  }
}

export const i18n = new I18nState();

export function isLocale(value: string): value is Locale {
  return (locales as readonly string[]).includes(value);
}
