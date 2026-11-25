
import { useSyncExternalStore } from "react";

/*
 * Post-hydration media-query match. Returns `false` on the server and during the
 * first client (hydration) render — so it never causes a hydration mismatch —
 * then updates to the real value and on later viewport changes. Used to gate
 * heavy desktop-only visuals (WebGL/video) off phones without shipping them to
 * SSR/mobile in the first place.
 */
export function useMediaQuery(query: string): boolean {
  return useSyncExternalStore(
    (onChange) => {
      const mql = window.matchMedia(query);
      mql.addEventListener("change", onChange);
      return () => mql.removeEventListener("change", onChange);
    },
    () => window.matchMedia(query).matches,
    () => false,
  );
}
