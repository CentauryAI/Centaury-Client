// Home answers "where do I continue?": the active project first (its agents + one
// button to open it), the other projects below as plain cards, and at most one row
// of figures — every one of them real (/v1/user/projects, /v1/org/members,
// /v1/org/agents, agents/max_agents from /v1/user/me). Demo runs the same path on mocks.
import { useCallback, useEffect, useState } from "react";
import { ArrowUpRight, Folder, Loader2, X } from "lucide-react";
import { QueryState } from "@/components/kore/QueryState";
import { Button, Eyebrow, PanelHeader, StatusDot } from "@/components/centaury";
import { api, errorInfo, store, type ErrorKind, type OrgAgent, type ProjectSummary } from "@/lib/api";
import { ago } from "@/lib/time";
import { cn } from "@/lib/utils";

type Fail = { kind: ErrorKind; message: string };
type Data = {
  projects: ProjectSummary[];
  members: number | null;
  agents: { used: number; max: number } | null;
  roster: OrgAgent[] | null; // null = could not load (shown, not hidden)
};

function Figure({ label, value, detail }: { label: string; value: string; detail?: string }) {
  return (
    <div className="min-w-0 rounded-2xl border border-border bg-card p-4">
      <Eyebrow>{label}</Eyebrow>
      <p className="mt-2 font-heading text-3xl font-semibold tracking-tight tabular-nums">{value}</p>
      {detail && <p className="mt-0.5 text-xs text-muted-foreground">{detail}</p>}
    </div>
  );
}

export type OpenError = { project: string; kind: ErrorKind; message: string };

export function Home({ onOpenProject, opening, openError, onDismissError, onLogin }: {
  onOpenProject: (name: string) => void;
  opening?: string | null;
  openError?: OpenError | null;
  onDismissError?: () => void;
  onLogin?: () => void;
}) {
  const [data, setData] = useState<Data | null>(null);
  const [err, setErr] = useState<Fail | null>(null);

  const load = useCallback(() => {
    const session = store.session ?? "";
    if (!store.demo && !session) return;
    setErr(null);
    setData(null);
    // projects are the page; the rest only adds figures, so its failure degrades that piece
    Promise.all([
      api.userProjects(session),
      api.orgMembers(session).then((m) => m.length).catch(() => null),
      api.me(session).then((me) => (me.agents != null && me.max_agents != null ? { used: me.agents, max: me.max_agents } : null)).catch(() => null),
      api.orgAgents(session).catch(() => null),
    ])
      .then(([projects, members, agents, roster]) => setData({ projects, members, agents, roster }))
      .catch((e) => setErr(errorInfo(e)));
  }, []);
  useEffect(load, [load]);

  const forbidden = openError?.kind === "forbidden";
  const projects = data?.projects ?? [];
  const current = projects.find((p) => p.name === store.project) ?? projects[0];
  const others = projects.filter((p) => p !== current);
  const agentsHere = (data?.roster ?? []).filter((a) => a.project === current?.name);
  const figures = data
    ? [
        { label: "Projects", value: String(projects.length) },
        data.members != null && { label: "Members", value: String(data.members) },
        data.agents && { label: "Agents", value: String(data.agents.used), detail: `of ${data.agents.max} allowed` },
      ].filter((f): f is { label: string; value: string; detail?: string } => !!f)
    : [];

  return (
    <div className="@container h-full min-w-0 overflow-y-auto">
      <div className="flex min-w-0 flex-col gap-5 p-2">
        <header className="flex min-h-8 items-center">
          <h1 className="text-ink font-heading text-lg font-semibold tracking-tight">Home</h1>
        </header>

        {openError && (
          <div role="alert" className="flex items-start gap-3 rounded-2xl border border-border bg-card p-4 text-sm">
            <div className="min-w-0 flex-1">
              <p className="font-medium">
                {forbidden ? `You don't belong to “${openError.project}”` : `Couldn't open “${openError.project}”`}
              </p>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {forbidden ? "Ask an organization admin to add you to this project." : openError.message}
              </p>
              {openError.kind === "auth" && onLogin && (
                <button type="button" onClick={onLogin} className="mt-2 text-xs font-medium text-primary underline">Sign in again</button>
              )}
              {!forbidden && openError.kind !== "auth" && (
                <button type="button" onClick={() => onOpenProject(openError.project)} className="mt-2 text-xs font-medium text-primary underline">Retry</button>
              )}
            </div>
            <button type="button" aria-label="Dismiss" onClick={onDismissError} className="rounded-full p-1 text-muted-foreground hover:bg-muted hover:text-foreground">
              <X size={14} />
            </button>
          </div>
        )}

        {!data ? (
          <QueryState
            state={err ? "error" : "loading"}
            message={err ? err.message : "Loading projects…"}
            kind={err?.kind}
            onRetry={load}
            onLogin={onLogin}
          />
        ) : projects.length === 0 ? (
          <QueryState state="empty" message="No projects yet. Ask an organization admin to add you to one." />
        ) : (
          <>
            {figures.length > 0 && (
              <div className="grid gap-3 @md:grid-cols-3">
                {figures.map((f) => <Figure key={f.label} {...f} />)}
              </div>
            )}

            <section aria-label={`Continue in ${current.name}`} className="rounded-2xl border border-border bg-card p-5">
              <Eyebrow>Continue in {current.name}</Eyebrow>
              <div className="mt-3 flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate font-heading text-2xl font-semibold tracking-tight">{current.name}</h2>
                  <p className="text-xs text-muted-foreground">
                    {current.instance_count} {current.instance_count === 1 ? "instance" : "instances"}
                  </p>
                </div>
                <Button onClick={() => onOpenProject(current.name)} disabled={!!opening} aria-busy={opening === current.name}>
                  {opening === current.name ? <Loader2 className="animate-spin" /> : <ArrowUpRight />} Open project
                </Button>
              </div>
              <div className="mt-4 border-t border-border/70 pt-3">
                {data.roster === null ? (
                  <p className="text-sm text-muted-foreground">Couldn't load the agents of this project.</p>
                ) : agentsHere.length === 0 ? (
                  <p className="text-sm text-muted-foreground">No agents have joined this project yet.</p>
                ) : (
                  <ul className="grid gap-x-6 gap-y-2 @lg:grid-cols-2">
                    {agentsHere.slice(0, 6).map((a) => (
                      <li key={a.id} className="flex min-w-0 items-center justify-between gap-3 text-sm">
                        <StatusDot tone={a.status === "active" ? "ok" : "idle"}>
                          <span className="truncate font-medium">{a.name}</span>
                        </StatusDot>
                        <span className="shrink-0 text-xs text-muted-foreground">{ago(a.last_seen)}</span>
                      </li>
                    ))}
                    {agentsHere.length > 6 && (
                      <li className="text-xs text-muted-foreground">+{agentsHere.length - 6} more</li>
                    )}
                  </ul>
                )}
              </div>
            </section>

            {others.length > 0 && (
              <section>
                <PanelHeader title="Other projects" />
                <div className="mt-3 grid gap-3 @sm:grid-cols-2 @2xl:grid-cols-3">
                  {others.map((p) => (
                    <button
                      key={p.name}
                      type="button"
                      disabled={!!opening}
                      aria-busy={opening === p.name}
                      onClick={() => onOpenProject(p.name)}
                      className={cn(
                        "group flex min-w-0 items-center gap-3 rounded-2xl border border-border bg-card p-4 text-left transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:outline-primary",
                        opening && opening !== p.name && "opacity-60",
                      )}
                    >
                      <Folder size={16} className="shrink-0 text-primary" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-heading text-base font-semibold tracking-tight">{p.name}</span>
                        <span className="text-xs text-muted-foreground">
                          {p.instance_count} {p.instance_count === 1 ? "instance" : "instances"}
                        </span>
                      </span>
                      {opening === p.name ? (
                        <Loader2 size={14} className="shrink-0 animate-spin" />
                      ) : (
                        <ArrowUpRight size={14} className="shrink-0 text-muted-foreground group-hover:text-foreground" />
                      )}
                    </button>
                  ))}
                </div>
              </section>
            )}
          </>
        )}
      </div>
    </div>
  );
}
