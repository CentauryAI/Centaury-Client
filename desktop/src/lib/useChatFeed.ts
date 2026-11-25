// One feed + one send path for every composer in a project (the central Chats
// composer and the side broadcast). Owns: history load (loading / empty /
// error), live WS with reconnect, optimistic outgoing bubbles, dedupe by id.
//
// Server facts this relies on (judge §0): the WS peek never echoes your own
// messages, so the echo comes from the POST response ({id, scope, mentions});
// peek has no replay, so history is re-fetched on every (re)connect and when
// the window regains focus; POST /v1/messages is NOT idempotent, so a failed
// send is never retried automatically — the user presses retry.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, connectWs, errorInfo, store, type Delivery, type ErrorKind, type InstanceSummary, type OrgMessage, type WsState } from "@/lib/api";

export type OutStatus = "sending" | "failed";
export type Outgoing = {
  key: string;
  text: string;
  targets: string[];
  reply_to?: number;
  thread?: string;
  status: OutStatus;
  error?: string;
  errorKind?: ErrorKind;
};
export type FeedLoad = "loading" | "ready" | "error";

let seq = 0;

export type SendOpts = { reply_to?: number; thread?: string };

// roster is only read to derive sender_kind of paged-in history (the org route lacks it)
export function useChatFeed(roster: InstanceSummary[] = []) {
  const [feed, setFeed] = useState<Delivery[]>([]);
  // message id → epoch ms. History: server created_at (org route, session token).
  // Live: local receive time. If the org route fails (401/403) the chat works without hours.
  const [times, setTimes] = useState<Record<number, number>>({});
  const [timesOk, setTimesOk] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [olderError, setOlderError] = useState<string | null>(null);
  const rosterRef = useRef(roster);
  rosterRef.current = roster;
  const feedRef = useRef<Delivery[]>([]);
  const [outbox, setOutbox] = useState<Outgoing[]>([]);
  const [load, setLoad] = useState<FeedLoad>("loading");
  const [loadError, setLoadError] = useState<{ kind: ErrorKind; message: string } | null>(null);
  const [conn, setConn] = useState<WsState>("connecting");
  const me = store.name ?? "";
  const alive = useRef(true);

  // merge by id, keep ascending order
  const merge = useCallback((incoming: Delivery[], live = false) => {
    const known = new Set(feedRef.current.map((m) => m.id));
    const fresh = incoming.filter((m, i) => !known.has(m.id) && incoming.findIndex((x) => x.id === m.id) === i);
    if (!fresh.length) return;
    if (live) {
      const now = Date.now();
      setTimes((t) => ({ ...Object.fromEntries(fresh.map((m) => [m.id, now])), ...t }));
    }
    setFeed((f) => {
      const have = new Set(f.map((m) => m.id));
      const add = fresh.filter((m) => !have.has(m.id));
      const next = add.length ? [...f, ...add].sort((a, b) => a.id - b.id) : f;
      feedRef.current = next;
      return next;
    });
  }, []);

  const applyTimes = useCallback((rows: OrgMessage[]) => {
    setTimes((t) => {
      const next = { ...t };
      for (const r of rows) {
        const ms = Date.parse(r.created_at);
        if (!Number.isNaN(ms)) next[r.id] = ms; // server time wins over local receive time
      }
      return next;
    });
  }, []);

  const reload = useCallback(
    async (initial = false) => {
      if (initial) {
        setLoad("loading");
        setLoadError(null);
      }
      try {
        const project = store.project ?? "";
        const [h, org] = await Promise.all([
          api.history(200),
          store.demo || store.session
            ? api.orgMessages(project, 200).then((r) => r, () => null)
            : Promise.resolve(null),
        ]);
        if (!alive.current) return;
        if (org) {
          applyTimes(org);
          setTimesOk(true);
        } else setTimesOk(false);
        merge(h);
        if (initial) setHasMore(!!org && org.length > 0);
        setLoad("ready");
        setLoadError(null);
      } catch (e) {
        if (!alive.current) return;
        // a failed background refresh must not wipe an already-loaded feed
        setLoad((cur) => (cur === "ready" ? cur : "error"));
        setLoadError(errorInfo(e));
      }
    },
    [merge, applyTimes],
  );

  /** page backwards through the org route (`before` = oldest id we hold). Position is the caller's job. */
  const loadOlder = useCallback(async () => {
    const oldest = feedRef.current[0]?.id;
    if (!oldest || loadingOlder) return false;
    setLoadingOlder(true);
    setOlderError(null);
    try {
      const rows = await api.orgMessages(store.project ?? "", 50, oldest);
      if (!alive.current) return false;
      const kinds = Object.fromEntries(rosterRef.current.map((i) => [i.name, i.kind]));
      applyTimes(rows);
      merge(
        rows.map((r) => ({
          id: r.id, from: r.from, sender_kind: kinds[r.from] ?? "agent", scope: r.scope, text: r.text, mentions: r.mentions ?? [],
          delivered_to: [], intent: r.intent, thread: r.thread, reply_to: r.reply_to, bundle_id: null,
        })),
      );
      setHasMore(rows.length >= 50);
      return rows.length > 0;
    } catch (e) {
      setOlderError(errorInfo(e).message);
      return false;
    } finally {
      setLoadingOlder(false);
    }
  }, [applyTimes, merge, loadingOlder]);

  useEffect(() => {
    alive.current = true;
    reload(true);
    let first = true;
    const off = connectWs(
      (d) => merge([d], true),
      (s) => {
        setConn(s);
        // peek has no replay: every transition to connected re-syncs (the first one is the initial load)
        if (s === "connected") {
          if (first) first = false;
          else reload();
        }
      },
    );
    const onFocus = () => {
      if (document.visibilityState === "visible") reload();
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onFocus);
    return () => {
      alive.current = false;
      off();
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onFocus);
    };
  }, [merge, reload]);

  const post = useCallback(
    async (key: string, text: string, targets: string[], o: SendOpts) => {
      try {
        const r = await api.send(text, targets, o.reply_to, o.thread);
        if (!alive.current) return;
        // sent: becomes a real message under the server id (WS/history dedupe on it)
        merge([
          {
            id: r.id, from: me, sender_kind: "human", scope: r.scope, text, mentions: r.mentions ?? targets,
            delivered_to: [], intent: null, thread: o.thread ?? null, reply_to: o.reply_to ?? null, bundle_id: null,
          },
        ], true);
        setOutbox((o) => o.filter((x) => x.key !== key));
      } catch (e) {
        if (!alive.current) return;
        const info = errorInfo(e);
        setOutbox((o) => o.map((x) => (x.key === key ? { ...x, status: "failed", error: info.message, errorKind: info.kind } : x)));
      }
    },
    [me, merge],
  );

  /** the single send path — optimistic bubble, then POST. Returns false for blank text. */
  const send = useCallback(
    (raw: string, targets: string[] = [], opts: SendOpts = {}) => {
      const text = raw.trim();
      if (!text) return false;
      const key = `out-${++seq}`;
      setOutbox((o) => [...o, { key, text, targets, reply_to: opts.reply_to, thread: opts.thread, status: "sending" }]);
      void post(key, text, targets, opts);
      return true;
    },
    [post],
  );

  /** manual retry of a failed bubble (never automatic: the server is not idempotent) */
  const retry = useCallback(
    (key: string) => {
      const item = outbox.find((x) => x.key === key);
      if (!item || item.status !== "failed") return;
      setOutbox((o) => o.map((x) => (x.key === key ? { ...x, status: "sending", error: undefined } : x)));
      void post(key, item.text, item.targets, { reply_to: item.reply_to, thread: item.thread });
    },
    [outbox, post],
  );

  const discard = useCallback((key: string) => setOutbox((o) => o.filter((x) => x.key !== key)), []);

  return useMemo(
    () => ({
      feed, outbox, load, loadError, conn, send, retry, discard, reload: () => reload(true),
      times, timesOk, hasMore, loadingOlder, olderError, loadOlder,
    }),
    [feed, outbox, load, loadError, conn, send, retry, discard, reload, times, timesOk, hasMore, loadingOlder, olderError, loadOlder],
  );
}
export type ChatFeed = ReturnType<typeof useChatFeed>;
