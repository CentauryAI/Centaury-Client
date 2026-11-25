// Chats — messaging inside the project bubble. Left: conversation list (Global
// pinned, then one row per instance ordered by last activity, last message line,
// local unread badge). Right: header (identity + presence as text), the message
// list (Markdown, avatars, day separators, anchored scroll, older-history paging),
// and the ONE composer. Every send goes through useChatFeed().send.
//
// Server facts (judge): peek never echoes your own messages (POST id does), history
// carries no timestamps (org route does, see useChatFeed), unread has no server
// read-state → last-read id per conversation lives in localStorage and only
// advances while the conversation is open AND the window is focused.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, Check, ChevronLeft, CornerUpLeft, Loader2, Megaphone, MessageSquare, RotateCw, X } from "lucide-react";
import { api, errorInfo, store, type Delivery, type ErrorKind, type InstanceSummary, type ThreadSummary } from "@/lib/api";
import type { ChatFeed } from "@/lib/useChatFeed";
import { QueryState } from "@/components/kore/QueryState";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";
import { Markdown, CopyButton } from "@/components/kore/Markdown";
import { Composer, type ComposerMode, type ReplyTarget } from "@/components/kore/Composer";
import { StatusDot, TabPills } from "@/components/centaury";
import { cn } from "@/lib/utils";
import { toast } from "sonner";
import { getPref } from "@/lib/prefs";
import { setUnreadTotal } from "@/lib/unread";

const GLOBAL = "__global__";
const TABS = [{ label: "All" }, { label: "Announcements" }, { label: "Directed" }, { label: "Threads" }] as const;
type Tab = (typeof TABS)[number]["label"];
const GROUP_GAP_MS = 5 * 60_000;

function isBroadcast(m: Delivery) {
  return m.scope === "broadcast" || (m.mentions ?? []).length === 0;
}
function hhmm(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
function dayKey(ms: number) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}
function dayLabel(ms: number) {
  const now = new Date();
  const d = new Date(ms);
  const diff = Math.round((new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() - new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()) / 86_400_000);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  return d.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short", ...(d.getFullYear() !== now.getFullYear() && { year: "numeric" }) });
}
const oneLine = (t: string) => t.replace(/[`*_>#|-]+/g, " ").replace(/\s+/g, " ").trim();

// ---- local read state ----
type ReadMap = Record<string, number>;
function useReadState(project: string) {
  const key = `kore.read.${project}`;
  const [read, setRead] = useState<ReadMap | null>(() => {
    try { return JSON.parse(localStorage.getItem(key) || "null"); } catch { return null; }
  });
  const readRef = useRef(read);
  readRef.current = read;
  const commit = useCallback((next: ReadMap) => {
    readRef.current = next;
    setRead(next);
    localStorage.setItem(key, JSON.stringify(next));
  }, [key]);
  return { read, commit, readRef };
}

function useWindowFocus() {
  const [f, setF] = useState(() => document.hasFocus());
  useEffect(() => {
    const on = () => setF(document.hasFocus());
    window.addEventListener("focus", on);
    window.addEventListener("blur", on);
    document.addEventListener("visibilitychange", on);
    return () => {
      window.removeEventListener("focus", on);
      window.removeEventListener("blur", on);
      document.removeEventListener("visibilitychange", on);
    };
  }, []);
  return f;
}

export function Chats({ roster, chat, project, onLogin, visible = true }: { roster: InstanceSummary[]; chat: ChatFeed; project: string; onLogin?: () => void; visible?: boolean }) {
  const { feed, outbox, load, loadError, send: sendMsg, times } = chat;
  const [active, setActive] = useState(GLOBAL);
  const [tab, setTab] = useState<Tab>("All");
  const [openThread, setOpenThread] = useState<string | null>(null);
  const [replyTo, setReplyTo] = useState<ReplyTarget | null>(null);
  const [seed, setSeed] = useState({ text: "", n: 0 });
  const me = store.name ?? "";
  const byName = useMemo(() => Object.fromEntries(roster.map((i) => [i.name, i])), [roster]);
  const positions = useRef(new Map<string, { top: number; bottom: boolean }>());
  // hidden workspace (user is on Home/Settings…) counts as not looking at the chat
  const focused = useWindowFocus() && visible;
  const { read, commit, readRef } = useReadState(project);

  // ---- per-conversation summaries (last message, unread) ----
  const maxId = feed.length ? feed[feed.length - 1].id : 0;
  useEffect(() => {
    // first ever visit: everything already there counts as read (baseline)
    if (load === "ready" && readRef.current === null) commit({ __base: maxId });
  }, [load, maxId, commit, readRef]);
  const lastRead = (conv: string) => read?.[conv] ?? read?.__base ?? Number.MAX_SAFE_INTEGER;

  const summaries = useMemo(() => {
    const out: Record<string, { last?: Delivery; unread: number }> = {};
    const get = (k: string) => (out[k] ??= { unread: 0 });
    const g = get(GLOBAL);
    for (const m of feed) {
      const mine = m.from === me;
      const ms = m.mentions ?? [];
      g.last = m;
      if (!mine && !(ms.length > 0 && ms.includes(me) && ms.length === 1) && m.id > lastRead(GLOBAL)) g.unread++;
      const peers = mine ? ms : ms.includes(me) ? [m.from] : [];
      for (const p of peers) {
        if (p === me) continue;
        const s = get(p);
        s.last = m;
        if (!mine && m.id > lastRead(p)) s.unread++;
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [feed, me, read]);

  // what the sidebar shows: the sum of the badges of the rows below
  const shownUnread = (k: string) => (active === k && focused ? 0 : summaries[k]?.unread ?? 0);
  const unreadTotal = Object.keys(summaries).reduce((n, k) => n + shownUnread(k), 0);
  useEffect(() => {
    setUnreadTotal(unreadTotal);
  }, [unreadTotal]);
  useEffect(() => () => setUnreadTotal(0), []);

  // notice (Settings → Notifications): a message landed in a conversation you are not looking at
  const seenUnread = useRef<Record<string, number> | null>(null);
  useEffect(() => {
    if (load !== "ready") return;
    const now: Record<string, number> = {};
    for (const k of Object.keys(summaries)) now[k] = shownUnread(k);
    const prev = seenUnread.current;
    seenUnread.current = now;
    if (!prev || !getPref("notify")) return;
    for (const k of Object.keys(now)) {
      if (now[k] > (prev[k] ?? 0)) {
        const last = summaries[k]?.last;
        const where = k === GLOBAL ? "Global" : k;
        toast(`New message in ${where}`, { description: last ? `${last.from}: ${oneLine(last.text).slice(0, 90)}` : undefined });
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [summaries, active, focused, load]);

  // mark the open conversation read — only with the window focused
  useEffect(() => {
    if (!focused || load !== "ready" || readRef.current === null) return;
    const conv = active;
    const lastId = conv === GLOBAL ? maxId : summaries[conv]?.last?.id ?? 0;
    if (lastId > (readRef.current[conv] ?? readRef.current.__base ?? 0)) commit({ ...readRef.current, [conv]: lastId });
  }, [focused, load, active, maxId, summaries, commit, readRef]);

  // ---- the open conversation ----
  const dm = useMemo(
    () => (active === GLOBAL ? feed : feed.filter((m) => (m.from === active && (m.mentions ?? []).includes(me)) || (m.from === me && (m.mentions ?? []).includes(active)))),
    [feed, active, me],
  );
  // thread: fetched list ∪ live feed
  const [threadMsgs, setThreadMsgs] = useState<Delivery[]>([]);
  const [threadState, setThreadState] = useState<{ s: "loading" | "ready" | "error"; kind?: ErrorKind; message?: string }>({ s: "ready" });
  const loadThread = useCallback((name: string) => {
    setThreadState({ s: "loading" });
    api.history(200, name)
      .then((r) => { setThreadMsgs(r); setThreadState({ s: "ready" }); })
      .catch((e) => setThreadState({ s: "error", ...errorInfo(e) }));
  }, []);
  useEffect(() => {
    if (openThread) { setThreadMsgs([]); loadThread(openThread); }
  }, [openThread, loadThread]);
  const threadConv = useMemo(() => {
    if (!openThread) return [];
    const seen = new Set<number>();
    return [...threadMsgs, ...feed.filter((m) => m.thread === openThread)]
      .filter((m) => (seen.has(m.id) ? false : (seen.add(m.id), true)))
      .sort((a, b) => a.id - b.id);
  }, [threadMsgs, feed, openThread]);

  const inGlobal = active === GLOBAL;
  const inThreadsTab = inGlobal && tab === "Threads";
  const conv = openThread && inThreadsTab
    ? threadConv
    : inGlobal
      ? tab === "Announcements" ? dm.filter(isBroadcast) : tab === "Directed" ? dm.filter((m) => !isBroadcast(m)) : dm
      : dm;
  const pending = useMemo(
    () => (inGlobal ? (openThread && inThreadsTab ? outbox.filter((o) => o.thread === openThread) : outbox) : outbox.filter((o) => o.targets.includes(active))),
    [outbox, active, inGlobal, openThread, inThreadsTab],
  );
  const convKey = `${active}|${inGlobal ? tab : ""}|${openThread ?? ""}`;

  const mode: ComposerMode = inGlobal ? (openThread && inThreadsTab ? { kind: "thread", name: openThread } : { kind: "global" }) : { kind: "dm", peer: active };

  function pickConversation(c: string) {
    setActive(c);
    setReplyTo(null);
    setOpenThread(null);
  }
  function startReply(m: Delivery) {
    setReplyTo({ id: m.id, from: m.from, snippet: oneLine(m.text).slice(0, 80) });
    if (inGlobal && m.from !== me && !openThread) setSeed((s) => ({ text: `@${m.from} `, n: s.n + 1 }));
    else setSeed((s) => ({ text: "", n: s.n + 1 }));
  }
  function openThreadByName(name: string) {
    setActive(GLOBAL);
    setTab("Threads");
    setOpenThread(name);
  }
  function onSend(text: string, targets: string[]) {
    const thread = openThread && inThreadsTab ? openThread : replyTo ? feed.find((m) => m.id === replyTo.id)?.thread ?? undefined : undefined;
    const ok = sendMsg(text, targets, { reply_to: replyTo?.id, thread });
    if (ok) setReplyTo(null);
    return ok;
  }

  // ---- conversation list ----
  const rows = useMemo(() => {
    const list = roster.filter((i) => i.name !== me);
    return list.sort((a, b) => (summaries[b.name]?.last?.id ?? 0) - (summaries[a.name]?.last?.id ?? 0) || a.name.localeCompare(b.name));
  }, [roster, me, summaries]);
  const rowLine = (k: string) => {
    const l = summaries[k]?.last;
    if (!l) return k === GLOBAL ? `Everyone in ${project}` : "No messages yet";
    return `${l.from === me ? "You: " : k === GLOBAL ? `${l.from}: ` : ""}${oneLine(l.text)}`;
  };

  const peer = !inGlobal ? byName[active] : undefined;

  return (
    <div className="flex min-h-0 flex-1">
      <aside aria-label="Conversations" className="flex w-48 shrink-0 flex-col overflow-y-auto border-r border-border">
        <ChatRow
          selected={inGlobal}
          onClick={() => pickConversation(GLOBAL)}
          avatar={<span className="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-muted"><Megaphone className="h-3.5 w-3.5 text-muted-foreground" /></span>}
          title="Global"
          line={rowLine(GLOBAL)}
          unread={shownUnread(GLOBAL)}
        />
        {rows.map((i) => (
          <ChatRow
            key={i.name}
            selected={active === i.name}
            onClick={() => pickConversation(i.name)}
            avatar={i.kind === "agent" ? <PixelSprite tool={i.tool ?? "claude"} size={32} /> : <AgentAvatar seed={i.name} size={32} className="rounded-full" />}
            title={i.name}
            tag={i.tag}
            line={rowLine(i.name)}
            tooltip={`${i.status === "active" ? "Connected" : "Not listening"}${i.status_context ? ` · ${i.status_context}` : ""}`}
            dot={i.status === "active"}
            unread={shownUnread(i.name)}
          />
        ))}
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        {/* header: identity + presence as text */}
        <div className="flex min-h-[52px] items-center gap-3 border-b border-border px-3 py-2">
          {inGlobal ? (
            <>
              <h2 className="sr-only">Global</h2>
              <div className="min-w-0">
                <TabPills tabs={TABS} value={tab} onChange={(t) => { setTab(t); setOpenThread(null); setReplyTo(null); }} />
              </div>
            </>
          ) : (
            <>
              {peer?.kind === "agent" ? <PixelSprite tool={peer.tool ?? "claude"} size={28} /> : <AgentAvatar seed={active} size={28} className="rounded-full" />}
              <div className="min-w-0 flex-1">
                <div className="flex items-baseline gap-2">
                  <h2 className="truncate text-sm font-semibold">{active}</h2>
                  {peer?.tag && <span className="shrink-0 text-xs text-muted-foreground">{peer.tag}</span>}
                  {peer?.tool && <span className="shrink-0 text-xs text-muted-foreground">{peer.tool}</span>}
                </div>
                <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
                  <StatusDot tone={peer?.status === "active" ? "ok" : "idle"}>
                    {peer?.status === "active" ? "Connected" : "Not listening"}
                  </StatusDot>
                  {peer?.status_context ? <span className="min-w-0 truncate" title={peer.status_context}>· {peer.status_context}</span> : null}
                </div>
              </div>
            </>
          )}
        </div>

        {inThreadsTab && !openThread ? (
          <ThreadList onOpen={setOpenThread} onLogin={onLogin} />
        ) : (
          <>
            {openThread && inThreadsTab && (
              <div className="flex items-center gap-2 border-b border-border px-3 py-1.5 text-sm">
                <button type="button" onClick={() => { setOpenThread(null); setReplyTo(null); }} className="inline-flex items-center gap-1 rounded-full px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground">
                  <ChevronLeft size={13} /> Threads
                </button>
                <span className="min-w-0 truncate font-medium">{openThread}</span>
              </div>
            )}
            {load === "loading" ? (
              <QueryState state="loading" message="Loading messages…" className="flex-1" />
            ) : load === "error" ? (
              <QueryState state="error" kind={loadError?.kind} message={loadError?.message} onRetry={chat.reload} onLogin={onLogin} className="flex-1" />
            ) : openThread && inThreadsTab && threadState.s === "loading" ? (
              <QueryState state="loading" message="Loading thread…" className="flex-1" />
            ) : openThread && inThreadsTab && threadState.s === "error" ? (
              <QueryState state="error" kind={threadState.kind} message={threadState.message} onRetry={() => loadThread(openThread)} onLogin={onLogin} className="flex-1" />
            ) : (
              <MessageList
                key={convKey}
                convKey={convKey}
                msgs={conv}
                pending={pending}
                chat={chat}
                me={me}
                byName={byName}
                feed={feed}
                times={times}
                showRoute={inGlobal && !openThread}
                canPage={!(openThread && inThreadsTab)}
                positions={positions.current}
                empty={inGlobal ? (tab === "All" ? `No messages yet. Say something to ${project}.` : `No ${tab.toLowerCase()} messages here.`) : `No messages with ${active} yet.`}
                onReply={startReply}
                onOpenThread={openThreadByName}
              />
            )}
            <Composer roster={roster} me={me} project={project} mode={mode} replyTo={replyTo} onCancelReply={() => setReplyTo(null)} onSend={onSend} seed={seed} />
          </>
        )}
      </div>
    </div>
  );
}

// ---- thread list (GET /v1/threads) ----
function ThreadList({ onOpen, onLogin }: { onOpen: (name: string) => void; onLogin?: () => void }) {
  const [state, setState] = useState<"loading" | "ready" | "error">("loading");
  const [err, setErr] = useState<{ kind: ErrorKind; message: string } | null>(null);
  const [rows, setRows] = useState<ThreadSummary[]>([]);
  const run = useCallback(() => {
    setState("loading");
    api.threads().then((r) => { setRows([...r].sort((a, b) => b.last_msg_id - a.last_msg_id)); setState("ready"); }).catch((e) => { setErr(errorInfo(e)); setState("error"); });
  }, []);
  useEffect(run, [run]);
  if (state === "loading") return <QueryState state="loading" message="Loading threads…" className="flex-1" />;
  if (state === "error") return <QueryState state="error" kind={err?.kind} message={err?.message} onRetry={run} onLogin={onLogin} className="flex-1" />;
  if (!rows.length) return <QueryState state="empty" message="No threads in this project yet." className="flex-1" />;
  return (
    <ul aria-label="Threads" className="min-h-0 flex-1 overflow-y-auto">
      {rows.map((t) => (
        <li key={t.name}>
          <button type="button" onClick={() => onOpen(t.name)} className="flex w-full items-center gap-3 border-b border-border/50 px-4 py-2.5 text-left transition-colors hover:bg-muted">
            <MessageSquare size={14} className="shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate text-sm font-medium">{t.name}</span>
            <span className="shrink-0 text-xs text-muted-foreground">{t.message_count} message{t.message_count === 1 ? "" : "s"}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}

// ---- message list with anchored scroll ----
function MessageList({ convKey, msgs, pending, chat, me, byName, feed, times, showRoute, canPage, positions, empty, onReply, onOpenThread }: {
  convKey: string;
  msgs: Delivery[];
  pending: ChatFeed["outbox"];
  chat: ChatFeed;
  me: string;
  byName: Record<string, InstanceSummary>;
  feed: Delivery[];
  times: Record<number, number>;
  showRoute: boolean;
  canPage: boolean;
  positions: Map<string, { top: number; bottom: boolean }>;
  empty: string;
  onReply: (m: Delivery) => void;
  onOpenThread: (name: string) => void;
}) {
  const sc = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const snap = useRef({ h: 0, top: 0 });
  const firstId = useRef<number | undefined>(undefined);
  const seenId = useRef(0);
  const [newCount, setNewCount] = useState(0);
  const lastId = msgs.length ? msgs[msgs.length - 1].id : 0;
  const byId = useMemo(() => new Map(feed.map((m) => [m.id, m])), [feed]);
  const { hasMore, loadingOlder, loadOlder, olderError, retry, discard } = chat;

  const toBottom = () => { const el = sc.current; if (el) el.scrollTop = el.scrollHeight; };

  // opening a conversation: restore where you were (bottom if you were following it / first time)
  useLayoutEffect(() => {
    const el = sc.current;
    if (!el) return;
    const p = positions.get(convKey);
    if (!p || p.bottom) { toBottom(); atBottom.current = true; } else { el.scrollTop = p.top; atBottom.current = false; }
    firstId.current = msgs[0]?.id;
    seenId.current = atBottom.current ? lastId : seenId.current;
    snap.current = { h: el.scrollHeight, top: el.scrollTop };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // content changed: keep reading position on prepend, follow if at bottom, else count what you missed
  const pendingSig = pending.map((o) => o.key + o.status).join();
  useLayoutEffect(() => {
    const el = sc.current;
    if (!el) return;
    if (firstId.current !== undefined && msgs[0] && msgs[0].id < firstId.current) {
      el.scrollTop = snap.current.top + (el.scrollHeight - snap.current.h);
    } else if (atBottom.current) {
      toBottom();
    }
    firstId.current = msgs[0]?.id;
    if (atBottom.current) { seenId.current = lastId; setNewCount(0); }
    else setNewCount(msgs.filter((m) => m.id > seenId.current && m.from !== me).length);
    snap.current = { h: el.scrollHeight, top: el.scrollTop };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [msgs.length, lastId]);
  // your own outgoing bubble always brings you to the bottom
  useLayoutEffect(() => {
    if (pending.length) { toBottom(); atBottom.current = true; seenId.current = lastId; setNewCount(0); }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending.length]);
  void pendingSig;

  function onScroll() {
    const el = sc.current;
    if (!el) return;
    const bottom = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
    atBottom.current = bottom;
    positions.set(convKey, { top: el.scrollTop, bottom });
    snap.current = { h: el.scrollHeight, top: el.scrollTop };
    if (bottom) { seenId.current = lastId; if (newCount) setNewCount(0); }
    if (el.scrollTop < 80 && canPage && hasMore && !loadingOlder) void loadOlder();
  }

  // grouping + separators
  const items: React.ReactNode[] = [];
  let prev: Delivery | undefined;
  const lastMine = [...msgs].reverse().find((m) => m.from === me)?.id;
  msgs.forEach((m) => {
    const t = times[m.id];
    const pt = prev ? times[prev.id] : undefined;
    const newDay = t !== undefined && (pt === undefined || dayKey(t) !== dayKey(pt));
    if (newDay) items.push(<DaySeparator key={`d${m.id}`} label={dayLabel(t)} />);
    const head = !prev || newDay || prev.from !== m.from || (t !== undefined && pt !== undefined && t - pt > GROUP_GAP_MS);
    items.push(
      <Bubble
        key={m.id}
        m={m}
        head={head}
        time={t}
        me={me}
        info={byName[m.from]}
        showRoute={showRoute}
        quote={m.reply_to ? byId.get(m.reply_to) : undefined}
        sent={m.id === lastMine}
        onReply={onReply}
        onOpenThread={onOpenThread}
      />,
    );
    prev = m;
  });

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div ref={sc} onScroll={onScroll} role="log" aria-label="Messages" className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-4 py-3">
        {canPage && (hasMore || loadingOlder || olderError) && (
          <div className="mb-2 flex justify-center">
            <button
              type="button"
              disabled={loadingOlder}
              onClick={() => void loadOlder()}
              className="inline-flex items-center gap-1.5 rounded-full border border-border px-3 py-1 text-xs text-muted-foreground hover:bg-muted disabled:opacity-60"
            >
              {loadingOlder && <Loader2 size={12} className="animate-spin" />}
              {olderError ? `Couldn't load earlier messages. Retry` : "Load earlier messages"}
            </button>
          </div>
        )}
        {msgs.length === 0 && pending.length === 0 ? (
          <QueryState state="empty" message={empty} />
        ) : (
          <div className="flex flex-col">
            {items}
            {pending.map((o) => (
              <div key={o.key} className="mt-2 flex flex-col items-end">
                <div className={cn("max-w-[min(78%,36rem)] rounded-2xl px-3.5 py-2 text-sm", o.status === "failed" ? "border border-destructive bg-card text-foreground" : "bg-foreground/70 text-background")}>
                  {o.targets.length > 0 && <div className="mb-0.5 text-[11px] opacity-70">→ {o.targets.join(", ")}</div>}
                  <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{o.text}</p>
                  {o.status === "sending" ? (
                    <span className="mt-0.5 flex items-center justify-end gap-1 text-[11px] opacity-70">
                      <Loader2 size={10} className="animate-spin" /> sending…
                    </span>
                  ) : (
                    <div role="alert" className="mt-1 text-xs">
                      <p className="text-destructive">Not sent: {o.error}</p>
                      <div className="mt-1.5 flex items-center justify-end gap-2">
                        <button type="button" onClick={() => retry(o.key)} className="inline-flex shrink-0 items-center gap-1 rounded-full border border-border px-2 py-0.5 hover:bg-muted">
                          <RotateCw size={11} /> Retry
                        </button>
                        <button type="button" onClick={() => discard(o.key)} aria-label="Discard unsent message" className="shrink-0 rounded-full p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground">
                          <X size={12} />
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
      {newCount > 0 && (
        <button
          type="button"
          onClick={() => { toBottom(); atBottom.current = true; seenId.current = lastId; setNewCount(0); }}
          className="absolute bottom-3 left-1/2 inline-flex -translate-x-1/2 items-center gap-1.5 rounded-full bg-foreground px-3.5 py-1.5 text-xs font-medium text-background shadow-lg"
        >
          <ArrowDown size={12} /> {newCount} new message{newCount === 1 ? "" : "s"}
        </button>
      )}
    </div>
  );
}

function DaySeparator({ label }: { label: string }) {
  return (
    <div role="separator" className="my-3 flex items-center gap-3 text-xs text-muted-foreground">
      <span className="h-px flex-1 bg-border" />
      <span>{label}</span>
      <span className="h-px flex-1 bg-border" />
    </div>
  );
}

function Avatar({ m, info }: { m: Delivery; info?: InstanceSummary }) {
  const agent = info ? info.kind === "agent" : m.sender_kind === "agent";
  return agent ? <PixelSprite tool={info?.tool ?? "claude"} size={32} /> : <AgentAvatar seed={m.from} size={32} className="rounded-full" />;
}

function Bubble({ m, head, time, me, info, showRoute, quote, sent, onReply, onOpenThread }: {
  m: Delivery; head: boolean; time?: number; me: string; info?: InstanceSummary; showRoute: boolean;
  quote?: Delivery; sent: boolean; onReply: (m: Delivery) => void; onOpenThread: (name: string) => void;
}) {
  const mine = m.from === me;
  const ms = m.mentions ?? [];
  const label = [
    m.intent && <span key="i" className={cn("rounded-full border px-1.5 py-px text-[10px] uppercase tracking-wide", mine ? "border-background/30" : "border-border text-muted-foreground")}>{m.intent}</span>,
    m.thread && (
      <button key="t" type="button" onClick={() => onOpenThread(m.thread!)} className="max-w-[10rem] truncate underline-offset-2 hover:underline">
        thread: {m.thread}
      </button>
    ),
    showRoute && !isBroadcast(m) && <span key="r">→ {ms.join(", ")}</span>,
  ].filter(Boolean);
  return (
    <div data-mid={m.id} className={cn("group flex gap-2.5", head ? "mt-3 first:mt-0" : "mt-0.5", mine && "justify-end")}>
      {!mine && <div className="w-8 shrink-0">{head && <Avatar m={m} info={info} />}</div>}
      <div className={cn("flex min-w-0 max-w-[min(78%,36rem)] flex-col", mine ? "items-end" : "items-start")}>
        {head && (
          <div className="mb-0.5 flex items-baseline gap-2 px-1">
            {!mine && (
              <>
                <span className="text-sm font-semibold">{m.from}</span>
                {info?.tag && <span className="text-[11px] text-muted-foreground">{info.tag}</span>}
              </>
            )}
            {time !== undefined && <time className="text-[11px] text-muted-foreground" dateTime={new Date(time).toISOString()}>{hhmm(time)}</time>}
          </div>
        )}
        <div className="relative max-w-full">
          <div className={cn("max-w-full rounded-2xl px-3.5 py-2", mine ? "bg-foreground text-background" : "bg-muted text-foreground")}>
            {quote || m.reply_to ? (
              <div className={cn("mb-1 flex items-center gap-1 border-l-2 pl-2 text-xs", mine ? "border-background/40 text-background/70" : "border-border text-muted-foreground")}>
                <CornerUpLeft size={11} className="shrink-0" />
                <span className="min-w-0 truncate">
                  {quote ? <><span className="font-medium">{quote.from}</span>: {oneLine(quote.text).slice(0, 70)}</> : `message #${m.reply_to}`}
                </span>
              </div>
            ) : null}
            {label.length > 0 && <div className={cn("mb-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11px]", mine ? "text-background/70" : "text-muted-foreground")}>{label}</div>}
            <Markdown text={m.text} />
            {sent && (
              <span className="mt-0.5 flex items-center justify-end gap-1 text-[11px] text-background/70">
                <Check size={10} /> <span>sent</span>
              </span>
            )}
          </div>
          <div className={cn("absolute -top-3 flex gap-0.5 rounded-lg border border-border bg-card p-0.5 opacity-0 shadow-sm transition-opacity focus-within:opacity-100 group-hover:opacity-100", mine ? "left-2" : "right-2")}>
            <CopyButton text={m.text} label="Copy message" />
            <button type="button" aria-label="Reply" title="Reply" onClick={() => onReply(m)} className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground">
              <CornerUpLeft size={12} />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function ChatRow({ selected, onClick, avatar, title, tag, line, dot, unread = 0, tooltip }: {
  selected: boolean;
  onClick: () => void;
  avatar: React.ReactNode;
  title: string;
  tag?: string | null;
  line: string;
  dot?: boolean;
  unread?: number;
  tooltip?: string;
}) {
  return (
    <button
      onClick={onClick}
      title={tooltip}
      aria-current={selected}
      className={cn("flex w-full items-center gap-2.5 border-b border-border/50 px-3 py-2.5 text-left transition-colors hover:bg-muted", selected && "bg-muted")}
    >
      {avatar}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1.5">
          <span className="truncate text-sm font-medium">{title}</span>
          {tag && <span className="shrink-0 text-[10px] text-muted-foreground">{tag}</span>}
          {dot !== undefined && <span className="ml-auto shrink-0 self-center"><StatusDot tone={dot ? "ok" : "idle"}><span className="sr-only">{dot ? "Connected" : "Not listening"}</span></StatusDot></span>}
        </div>
        <p className={cn("truncate text-xs", unread ? "text-foreground" : "text-muted-foreground")}>{line}</p>
      </div>
      {unread > 0 && (
        <span data-testid="unread" aria-label={`${unread} unread`} className="grid h-5 min-w-5 shrink-0 place-items-center rounded-full bg-foreground px-1.5 text-[11px] font-semibold text-background">
          {unread > 99 ? "99+" : unread}
        </span>
      )}
    </button>
  );
}
