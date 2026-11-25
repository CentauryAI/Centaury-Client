// Local, per-device preferences (localStorage, keys `kore.pref.*`). The server
// has no preferences route, so nothing here leaves the machine.
import { useSyncExternalStore } from "react";

const PREFIX = "kore.pref.";
const EVENT = "kore:pref";

export const PREF_DEFAULTS = {
  /** toast when a message lands in a conversation you are not looking at */
  notify: true,
  /** switch the app's own animations off (also honours prefers-reduced-motion) */
  reduceMotion: false,
} as const;
export type PrefKey = keyof typeof PREF_DEFAULTS;

export function getPref(key: PrefKey): boolean {
  try {
    const v = localStorage.getItem(PREFIX + key);
    return v === null ? PREF_DEFAULTS[key] : v === "1";
  } catch {
    return PREF_DEFAULTS[key];
  }
}

export function setPref(key: PrefKey, value: boolean) {
  localStorage.setItem(PREFIX + key, value ? "1" : "0");
  window.dispatchEvent(new Event(EVENT));
  applyPrefs();
}

function subscribe(cb: () => void) {
  window.addEventListener(EVENT, cb);
  window.addEventListener("storage", cb);
  return () => {
    window.removeEventListener(EVENT, cb);
    window.removeEventListener("storage", cb);
  };
}

export function usePref(key: PrefKey): [boolean, (v: boolean) => void] {
  const v = useSyncExternalStore(subscribe, () => getPref(key));
  return [v, (next) => setPref(key, next)];
}

/** `data-reduce-motion` on <html> is what index.css keys off. */
export function applyPrefs() {
  if (typeof document === "undefined") return;
  if (getPref("reduceMotion")) document.documentElement.dataset.reduceMotion = "1";
  else delete document.documentElement.dataset.reduceMotion;
}
applyPrefs();
window.addEventListener("storage", applyPrefs);
