// Total of local unread messages of the project on screen. Chats computes it
// (it owns the per-conversation read state) and the sidebar shows it.
import { useSyncExternalStore } from "react";

let total = 0;
const subs = new Set<() => void>();

export function setUnreadTotal(n: number) {
  if (n === total) return;
  total = n;
  subs.forEach((f) => f());
}

export function useUnreadTotal() {
  return useSyncExternalStore(
    (cb) => (subs.add(cb), () => void subs.delete(cb)),
    () => total,
  );
}
