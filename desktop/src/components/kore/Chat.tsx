import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { api, connectWs, store, type Delivery, type InstanceSummary } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Badge } from "@/components/ui/badge";
import { ThemeToggle } from "@/lib/theme";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { toast } from "sonner";
import { LogOut, Send } from "lucide-react";
import { cn } from "@/lib/utils";

export function Chat({ onLogout }: { onLogout: () => void }) {
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  const [feed, setFeed] = useState<Delivery[]>([]);
  const [text, setText] = useState("");
  const feedEnd = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.history(80).then((h) => setFeed(h.reverse())).catch((e) => toast.error(String(e)));
    const loadRoster = () => api.instances().then(setRoster).catch(() => {});
    loadRoster();
    const iv = setInterval(loadRoster, 5000);
    let disconnect: (() => void) | undefined;
    connectWs((d) => setFeed((f) => (f.some((m) => m.id === d.id) ? f : [...f, d]))).then((fn) => (disconnect = fn));
    return () => {
      clearInterval(iv);
      disconnect?.();
    };
  }, []);

  useEffect(() => {
    feedEnd.current?.scrollIntoView({ behavior: "smooth" });
  }, [feed]);

  async function send() {
    const t = text.trim();
    if (!t) return;
    setText("");
    try {
      // @mentions in the text become targets server-side (routing.rs).
      const targets = [...t.matchAll(/@([a-z0-9-]+)/gi)].map((m) => m[1]);
      const { id } = await api.send(t, targets);
      // optimistic echo — in demo there's no WS; in live the WS echo dedups by id.
      setFeed((f) =>
        f.some((m) => m.id === id)
          ? f
          : [...f, { id, from: store.name ?? "you", sender_kind: "human", scope: targets.length ? "mentions" : "broadcast", text: t, mentions: targets, delivered_to: [], intent: null, thread: null, reply_to: null, bundle_id: null }],
      );
    } catch (e) {
      toast.error(String(e));
      setText(t);
    }
  }

  return (
    <div className="flex h-screen bg-background text-foreground">
      {/* roster */}
      <aside className="flex w-64 flex-col border-r">
        <div className="flex items-center justify-between px-4 py-3">
          <div>
            <div className="text-sm font-semibold">{store.project}</div>
            <div className="text-xs text-muted-foreground">@{store.name}</div>
          </div>
          <ThemeToggle />
        </div>
        <Separator />
        <ScrollArea className="flex-1">
          <div className="p-2">
            {roster.map((i) => (
              <button
                key={i.name}
                onClick={() => setText((t) => `${t}@${i.tag ? `${i.tag}-` : ""}${i.name} `)}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent"
              >
                <div className="relative shrink-0">
                  <AgentAvatar seed={i.name} size={26} animated={i.status === "active"} className="rounded-full" />
                  <span
                    className={cn(
                      "absolute -bottom-0.5 -right-0.5 h-2.5 w-2.5 rounded-full border-2 border-background",
                      i.status === "active" ? "bg-emerald-500" : "bg-muted-foreground/40",
                    )}
                  />
                </div>
                <span className="flex-1 truncate">
                  {i.tag ? `${i.tag}-` : ""}
                  {i.name}
                </span>
                {i.kind === "human" && <Badge variant="secondary" className="text-[10px]">human</Badge>}
              </button>
            ))}
          </div>
        </ScrollArea>
        <Separator />
        <Button variant="ghost" className="justify-start rounded-none" onClick={onLogout}>
          <LogOut className="mr-2 h-4 w-4" /> Sign out
        </Button>
      </aside>

      {/* feed + composer */}
      <main className="flex flex-1 flex-col">
        <ScrollArea className="flex-1">
          <div className="mx-auto flex max-w-3xl flex-col gap-3 p-6">
            <AnimatePresence initial={false}>
              {feed.map((m) => (
                <motion.div
                  key={m.id}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  className={cn("flex gap-2", m.from === store.name ? "flex-row-reverse" : "flex-row")}
                >
                  {m.from !== store.name && (
                    <AgentAvatar seed={m.from} size={28} className="mt-5 shrink-0 rounded-full" />
                  )}
                  <div className={cn("flex min-w-0 flex-col", m.from === store.name ? "items-end" : "items-start")}>
                    <div className="mb-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
                      <span className="font-medium text-foreground">{m.from}</span>
                      {m.sender_kind !== "agent" && <Badge variant="outline" className="text-[10px]">{m.sender_kind}</Badge>}
                      <span>#{m.id}</span>
                    </div>
                    <div
                      className={cn(
                        "max-w-full whitespace-pre-wrap break-words rounded-2xl px-3.5 py-2 text-sm",
                        m.from === store.name ? "bg-primary text-primary-foreground" : "bg-muted",
                      )}
                    >
                      {m.text}
                    </div>
                  </div>
                </motion.div>
              ))}
            </AnimatePresence>
            <div ref={feedEnd} />
          </div>
        </ScrollArea>
        <div className="border-t p-4">
          <div className="mx-auto flex max-w-3xl items-center gap-2">
            <Input
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && (e.preventDefault(), send())}
              placeholder="Message  ·  @name to address"
            />
            <Button size="icon" onClick={send}>
              <Send className="h-4 w-4" />
            </Button>
          </div>
        </div>
      </main>
    </div>
  );
}
