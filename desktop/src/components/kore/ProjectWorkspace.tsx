// ProjectWorkspace — you clicked a project on Home, now you're inside its bubble
// (the isolated env, same idea as cd-ing to a folder and launching agents there).
// Layout: Chats (the messaging UI, one composer) + a roster side panel. The
// collaboration graph moved to its own page (Network) — not here anymore.
// Reads GET /v1/instances (mock in demo). The feed/WS is useChatFeed, owned here.
import { useEffect, useMemo, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { api, errorInfo, type ErrorKind, type InstanceSummary } from "@/lib/api";
import { useChatFeed } from "@/lib/useChatFeed";
import { QueryState } from "@/components/kore/QueryState";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";
import { AgentDetail } from "@/components/kore/AgentDetail";
import { SummonForm } from "@/components/kore/SummonForm";
import { Chats } from "@/components/kore/Chats";
import { IconButton, PanelHeader, StatusDot } from "@/components/centaury";
import Button from "@/components/centaury/primitives/Button";

export function ProjectWorkspace({ project, onBack, onLogin, visible = true }: { project: string; onBack: () => void; onLogin?: () => void; visible?: boolean }) {
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  const [rosterState, setRosterState] = useState<"loading" | "ready" | "error">("loading");
  const [rosterErr, setRosterErr] = useState<{ kind: ErrorKind; message: string } | null>(null);
  // one feed + one send path (the Chats composer); roster only types paged-in history
  const chat = useChatFeed(roster);
  const [detail, setDetail] = useState<InstanceSummary | null>(null);
  const [summon, setSummon] = useState(false);

  const loadRoster = () => {
    setRosterState((s) => (s === "ready" ? s : "loading"));
    return api
      .instances()
      .then((r) => {
        setRoster(r);
        setRosterState("ready");
        setRosterErr(null);
      })
      .catch((e) => {
        setRosterErr(errorInfo(e));
        setRosterState((s) => (s === "ready" ? s : "error"));
      });
  };
  useEffect(() => {
    loadRoster();
  }, [project]);
  // DU-D8 (DU-S5 v1): poll the roster while this project view is open —
  // status_context (idle/working) is the trustworthy liveness signal; the
  // presence dot alone lies (register hard-codes 'active', WS close flaps it).
  useEffect(() => {
    const t = setInterval(() => api.instances().then((r) => { setRoster(r); setRosterState("ready"); }).catch(() => {}), 3000);
    return () => clearInterval(t);
  }, [project]);

  const humans = useMemo(() => roster.filter((i) => i.kind === "human"), [roster]);
  const agents = useMemo(() => roster.filter((i) => i.kind === "agent"), [roster]);

  return (
    <div className="flex h-full flex-col">
      {/* Header row — PanelHeader language: back IconButton + project name in font-heading + Summon */}
      <header className="flex items-center gap-3 border-b border-border px-5 py-3">
        <IconButton label="Back to projects" onClick={onBack}>
          <ArrowLeft className="h-3.5 w-3.5" />
        </IconButton>
        <div className="min-w-0 flex-1">
          <PanelHeader title={project}>
            {/* count badge sits inside the PanelHeader children (right side) */}
          </PanelHeader>
          <p className="mt-0.5 text-xs text-muted-foreground">
            {agents.length} {agents.length === 1 ? "agent" : "agents"} · {humans.length} {humans.length === 1 ? "human" : "humans"}
          </p>
        </div>
        <ConnectionBadge state={chat.conn} />
        <Button
          size="sm"
          variant="primary"
          onClick={() => setSummon(true)}
        >
          Summon
        </Button>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* messaging */}
        <div className="flex min-w-0 flex-1 flex-col">
          <Chats roster={roster} chat={chat} project={project} onLogin={onLogin} visible={visible} />
        </div>

        {/* roster side panel — name first; tool and presence as text underneath */}
        <aside aria-label="Roster" className="flex w-48 shrink-0 flex-col border-l border-border">
          <div className="flex-1 overflow-y-auto p-3">
            <div className="rounded-2xl border border-border bg-card p-2.5">
              <p className="mb-3 font-mono text-[11px] uppercase tracking-[0.28em] text-muted-foreground">Roster</p>
              {rosterState === "loading" && <QueryState state="loading" message="Loading roster…" className="py-6" />}
              {rosterState === "error" && (
                <QueryState state="error" kind={rosterErr?.kind} message={rosterErr?.message} onRetry={loadRoster} onLogin={onLogin} className="py-6" />
              )}
              {rosterState === "ready" && roster.length === 0 && <QueryState state="empty" message="Nobody in this project yet." className="py-6" />}
              <div className="flex flex-col gap-3">
                {humans.map((h) => (
                  <div key={h.name}>
                    <RosterRow i={h} onClick={() => setDetail(h)} />
                    <div className="flex flex-col gap-0.5 pl-3">
                      {agents.filter((a) => a.owner === h.name).map((a) => (
                        <RosterRow key={a.name} i={a} onClick={() => setDetail(a)} />
                      ))}
                    </div>
                  </div>
                ))}
                {/* agents with no matching human in roster */}
                {agents.filter((a) => !humans.some((h) => h.name === a.owner)).map((a) => (
                  <RosterRow key={a.name} i={a} onClick={() => setDetail(a)} />
                ))}
              </div>
            </div>
          </div>
        </aside>
      </div>

      <AgentDetail agent={detail} onClose={() => setDetail(null)} onChanged={loadRoster} />
      <SummonForm project={project} open={summon} onClose={() => setSummon(false)} onLaunched={loadRoster} />
    </div>
  );
}

// WS connection indicator (green = live; amber pulse = reconnecting with backoff)
function ConnectionBadge({ state }: { state: "connecting" | "connected" | "reconnecting" }) {
  const live = state === "connected";
  return (
    <span
      role="status"
      title={live ? "Live: new messages arrive instantly" : "Connection lost: retrying with growing waits. History re-syncs when it returns."}
      className="inline-flex shrink-0 items-center rounded-full bg-muted px-2.5 py-1 text-xs text-muted-foreground"
    >
      <StatusDot tone={live ? "ok" : "pending"}>
        {live ? "Connected" : state === "connecting" ? "Connecting…" : "Reconnecting…"}
      </StatusDot>
    </span>
  );
}

// One roster row: the name leads, tag is secondary, tool + presence read as text.
// Presence = what the server says (active is set on every register), not "ready".
function RosterRow({ i, onClick }: { i: InstanceSummary; onClick: () => void }) {
  const on = i.status === "active";
  return (
    <button
      onClick={onClick}
      title={[i.tool, i.status_context].filter(Boolean).join(" · ") || undefined}
      className="flex w-full items-center gap-2 rounded-xl px-2 py-1.5 text-left transition-colors hover:bg-muted"
    >
      {i.kind === "agent" ? <PixelSprite tool={i.tool ?? "claude"} size={24} /> : <AgentAvatar seed={i.name} size={24} className="shrink-0 rounded-full" />}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-1.5">
          <span className="truncate text-sm font-medium">{i.name}</span>
          {i.tag && <span className="shrink-0 text-[10px] text-muted-foreground">{i.tag}</span>}
        </div>
        <p className="truncate text-[11px] text-muted-foreground">
          <StatusDot tone={on ? "ok" : "idle"}>
            {on ? "Connected" : "Not listening"}
          </StatusDot>
        </p>
      </div>
    </button>
  );
}
