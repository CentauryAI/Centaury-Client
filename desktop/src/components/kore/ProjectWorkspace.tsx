// ProjectWorkspace — you clicked a project on Home, now you're inside its bubble
// (the isolated env, same idea as cd-ing to a folder and launching agents there).
// Layout: Chats (the messaging UI) + a roster/broadcast side panel. The
// collaboration graph moved to its own page (Network) — not here anymore.
// Reads GET /v1/instances (mock in demo); broadcast POSTs /v1/messages with no
// targets. Chats owns its own message feed/WS.
import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Cpu, Megaphone, Rocket, Send } from "lucide-react";
import { api, type InstanceSummary } from "@/lib/api";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";
import { AgentDetail } from "@/components/kore/AgentDetail";
import { SummonForm } from "@/components/kore/SummonForm";
import { Chats } from "@/components/kore/Chats";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";
import { toast } from "sonner";

export function ProjectWorkspace({ project, onBack }: { project: string; onBack: () => void }) {
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  const [draft, setDraft] = useState("");
  const [detail, setDetail] = useState<InstanceSummary | null>(null);
  const [summon, setSummon] = useState(false);

  const loadRoster = () => api.instances().then(setRoster).catch((e) => toast.error(String(e)));
  useEffect(() => {
    loadRoster();
  }, [project]);
  // DU-D8 (DU-S5 v1): poll the roster while this project view is open —
  // status_context (idle/working) is the trustworthy liveness signal; the
  // presence dot alone lies (register hard-codes 'active', WS close flaps it).
  useEffect(() => {
    const t = setInterval(() => api.instances().then(setRoster).catch(() => {}), 3000);
    return () => clearInterval(t);
  }, [project]);

  const humans = useMemo(() => roster.filter((i) => i.kind === "human"), [roster]);
  const agents = useMemo(() => roster.filter((i) => i.kind === "agent"), [roster]);

  async function broadcast() {
    const text = draft.trim();
    if (!text) return;
    setDraft("");
    try {
      await api.send(text); // no targets = broadcast to the bubble
    } catch (e) {
      toast.error(String(e));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b px-6 py-4">
        <Button variant="ghost" size="icon" onClick={onBack} className="h-8 w-8 rounded-full">
          <ArrowLeft className="h-4 w-4" />
        </Button>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h1 className="k-title truncate text-lg">{project}</h1>
            <span className="rounded-full border border-dashed px-2 py-0.5 font-mono text-[10px] uppercase text-muted-foreground">isolated env</span>
          </div>
          <p className="text-xs text-muted-foreground">{agents.length} agents · {humans.length} humans</p>
        </div>
        <button onClick={() => setSummon(true)} className="k-pill-solid">
          <Rocket className="h-3.5 w-3.5" /> Summon
        </button>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* messaging */}
        <div className="flex min-w-0 flex-1 flex-col">
          <Chats roster={roster} />
        </div>

        {/* roster + broadcast side panel */}
        <aside className="flex w-80 shrink-0 flex-col border-l">
          <div className="flex-1 overflow-y-auto p-4">
            <h2 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">Roster</h2>
            <div className="flex flex-col gap-4">
              {humans.map((h) => (
                <div key={h.name}>
                  <button onClick={() => setDetail(h)} className="mb-1.5 flex w-full items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-accent">
                    <AgentAvatar seed={h.name} size={24} className="rounded-full" />
                    <span className="text-sm font-semibold">{h.name}</span>
                    <span className="text-[11px] text-muted-foreground">owner · {agents.filter((a) => a.owner === h.name).length} agents</span>
                  </button>
                  <div className="flex flex-col gap-1 border-l pl-3">
                    {agents.filter((a) => a.owner === h.name).map((a) => (
                      <button key={a.name} onClick={() => setDetail(a)} className="flex w-full items-center gap-2 rounded px-1 py-0.5 text-left hover:bg-accent">
                        <PixelSprite tool={a.tool ?? "claude"} size={22} />
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-1.5">
                            <span className="truncate text-xs font-medium">{a.tag ? `${a.tag}-` : ""}{a.name}</span>
                            <span className="inline-flex items-center gap-0.5 text-[10px] text-muted-foreground"><Cpu className="h-2.5 w-2.5" />{a.tool}</span>
                            {/* §2 rule 4: presence lies — never render inactive as dead nor active as ready */}
                            <span title={a.status === "active" ? "connected (not necessarily ready)" : "not listening right now (may be mid-turn)"} className={cn("h-1.5 w-1.5 rounded-full", a.status === "active" ? "bg-emerald-500" : "bg-muted-foreground/40")} />
                          </div>
                          <p className="truncate text-[10px] text-muted-foreground">{a.status_context || "—"}</p>
                        </div>
                      </button>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* broadcast composer */}
          <div className="border-t p-3">
            <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium text-muted-foreground">
              <Megaphone className="h-3.5 w-3.5" /> Broadcast to all agents
            </div>
            <Textarea
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  broadcast();
                }
              }}
              placeholder="Message the whole fleet…"
              className="min-h-[52px] resize-none text-sm"
            />
            <Button onClick={broadcast} disabled={!draft.trim()} className="mt-2 w-full" size="sm">
              <Send className="mr-2 h-3.5 w-3.5" /> Broadcast
            </Button>
          </div>
        </aside>
      </div>

      <AgentDetail agent={detail} onClose={() => setDetail(null)} onChanged={loadRoster} />
      <SummonForm project={project} open={summon} onClose={() => setSummon(false)} onLaunched={loadRoster} />
    </div>
  );
}
