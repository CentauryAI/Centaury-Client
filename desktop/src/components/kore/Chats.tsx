// Chats (DU-D5 v1) — WhatsApp-style messaging inside the project bubble.
// Left: chat list = Global + one row per instance (avatar, name, live
// status_context line). Right: the conversation.
//   Global (R13/R17): full project feed, history + live peek WS; non-broadcast
//   messages render "from → recipients" from scope+mentions (never
//   delivered_to — always empty on the wire, §2 rule 2); agent→its-owner pairs
//   get the DM affordance (R18, roster owner field). Composer = broadcast.
//   DM (R12): history filtered to {me ↔ x} via scope=mentions; composer sends
//   targets=[x] — same semantics as a CLI @mention.
// R14 (@ in global must not narrow routing) waits on DU-S3's plain flag —
// until then typed @s DO resolve server-side (noted in the composer hint).
import { useEffect, useMemo, useRef, useState } from "react";
import { Megaphone, Send } from "lucide-react";
import { api, connectWs, store, type Delivery, type InstanceSummary } from "@/lib/api";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

const GLOBAL = "__global__";

function isBroadcast(m: Delivery) {
  return m.scope === "broadcast" || (m.mentions ?? []).length === 0;
}

export function Chats({ roster }: { roster: InstanceSummary[] }) {
  const [feed, setFeed] = useState<Delivery[]>([]);
  const [active, setActive] = useState(GLOBAL);
  const [draft, setDraft] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  const me = store.name ?? "";
  const owners = useMemo(() => Object.fromEntries(roster.map((i) => [i.name, i.owner])), [roster]);

  useEffect(() => {
    api.history(100).then((h) => setFeed([...h].sort((a, b) => a.id - b.id))).catch(() => {});
    let off: (() => void) | undefined;
    connectWs((d) => setFeed((f) => (f.some((m) => m.id === d.id) ? f : [...f, d]))).then((fn) => (off = fn));
    return () => off?.();
  }, []);

  const conv = useMemo(() => {
    if (active === GLOBAL) return feed;
    return feed.filter(
      (m) =>
        (m.from === active && (m.mentions ?? []).includes(me)) ||
        (m.from === me && (m.mentions ?? []).includes(active))
    );
  }, [feed, active, me]);
  useEffect(() => endRef.current?.scrollIntoView({ behavior: "smooth" }), [conv.length, active]);

  async function send() {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    try {
      const targets = active === GLOBAL ? [] : [active];
      const { id } = await api.send(text, targets);
      setFeed((f) => [
        ...f,
        { id, from: me, sender_kind: "human", scope: targets.length ? "mentions" : "broadcast", text, mentions: targets, delivered_to: [], intent: null, thread: null, reply_to: null, bundle_id: null },
      ]);
    } catch (e) {
      toast.error(String(e));
    }
  }

  const rows = roster.filter((i) => i.name !== me);

  return (
    <div className="flex min-h-0 flex-1">
      {/* chat list */}
      <aside className="flex w-64 shrink-0 flex-col overflow-y-auto border-r">
        <ChatRow
          selected={active === GLOBAL}
          onClick={() => setActive(GLOBAL)}
          avatar={<span className="grid h-8 w-8 place-items-center rounded-full bg-primary/10"><Megaphone className="h-4 w-4" /></span>}
          title="Global"
          line="everyone in the project"
        />
        {rows.map((i) => (
          <ChatRow
            key={i.name}
            selected={active === i.name}
            onClick={() => setActive(i.name)}
            avatar={
              i.kind === "agent" ? (
                <PixelSprite tool={i.tool ?? "claude"} size={28} />
              ) : (
                <AgentAvatar seed={i.name} size={28} className="rounded-full" />
              )
            }
            title={`${i.tag ? `${i.tag}-` : ""}${i.name}`}
            line={i.status_context || (i.kind === "human" ? "owner" : "—")}
            dot={i.status === "active"}
          />
        ))}
      </aside>

      {/* conversation */}
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex-1 overflow-y-auto p-4">
          <div className="flex flex-col gap-2">
            {conv.map((m) => {
              const mine = m.from === me;
              const dm = !isBroadcast(m) && (m.mentions ?? []).some((r) => owners[m.from] === r || owners[r] === m.from);
              return (
                <div key={m.id} className={cn("max-w-[75%] rounded-lg px-3 py-2 text-sm", mine ? "self-end bg-primary text-primary-foreground" : "self-start bg-accent")}>
                  <div className={cn("mb-0.5 flex items-center gap-1 text-[11px]", mine ? "text-primary-foreground/70" : "text-muted-foreground")}>
                    <span className="font-medium">{m.from}</span>
                    {m.sender_kind !== "agent" && <span>[{m.sender_kind}]</span>}
                    {active === GLOBAL && !isBroadcast(m) && (
                      <span>→ {(m.mentions ?? []).join(", ")}{dm ? " · DM" : ""}</span>
                    )}
                  </div>
                  <p className="whitespace-pre-wrap break-words">{m.text}</p>
                </div>
              );
            })}
            <div ref={endRef} />
          </div>
        </div>
        <div className="border-t p-3">
          <Textarea
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            placeholder={active === GLOBAL ? "Message everyone… (@name still routes narrow until DU-S3)" : `Message ${active}…`}
            className="min-h-[48px] resize-none text-sm"
          />
          <Button onClick={send} disabled={!draft.trim()} size="sm" className="mt-2 w-full">
            <Send className="mr-2 h-3.5 w-3.5" /> {active === GLOBAL ? "Broadcast" : `Send to ${active}`}
          </Button>
        </div>
      </div>
    </div>
  );
}

function ChatRow({ selected, onClick, avatar, title, line, dot }: {
  selected: boolean;
  onClick: () => void;
  avatar: React.ReactNode;
  title: string;
  line: string;
  dot?: boolean;
}) {
  return (
    <button onClick={onClick} className={cn("flex w-full items-center gap-2.5 border-b px-3 py-2.5 text-left hover:bg-accent", selected && "bg-accent")}>
      {avatar}
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <span className="truncate text-sm font-medium">{title}</span>
          {dot !== undefined && (
            /* §2 rule 4: presence lies — dot ≠ ready/dead */
            <span className={cn("h-1.5 w-1.5 shrink-0 rounded-full", dot ? "bg-emerald-500" : "bg-muted-foreground/40")} />
          )}
        </div>
        <p className="truncate text-[11px] text-muted-foreground">{line}</p>
      </div>
    </button>
  );
}
