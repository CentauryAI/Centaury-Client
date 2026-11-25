
import { useMediaQuery } from "./useMediaQuery";

/*
 * `prefers-reduced-motion: reduce`, read after hydration. framer-motion's
 * `useReducedMotion` already knows the answer on the first client render, so a
 * component that renders differently with it (plain text instead of split
 * words) no longer matches the server HTML and React throws a hydration error
 * for every reduced-motion visitor. This returns `false` on the server and in
 * the hydration render, then the real value.
 */
export function useReducedMotionSafe(): boolean {
  return useMediaQuery("(prefers-reduced-motion: reduce)");
}
