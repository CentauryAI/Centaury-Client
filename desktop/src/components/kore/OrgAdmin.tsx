// OrgAdmin (DU-D10, R1/R3, stage 4) — org administration in the app. Role comes
// from GET /v1/user/me. Admin tabs: Accounts, Projects, Agents, Audit; member:
// Members + Agents (read-only, the server scopes /v1/org/agents). Risky account
// writes (role, disable, password) go through a confirmation Dialog. All writes
// ride the SESSION token; the server is the authority — a thin remote control.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, FolderPlus, KeyRound, RefreshCw, ShieldCheck, UserMinus, UserPlus } from "lucide-react";
import { api, errorInfo, store, type AuditEntry, type ErrorKind, type OrgAgent, type OrgMemberSummary, type OrgUsage, type OrgUserSummary, type ProjectMemberSummary } from "@/lib/api";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { Button, DataTable, Eyebrow, IconButton, Meter, PanelHeader, StatusDot, TabPills } from "@/components/centaury";
import { cn } from "@/lib/utils";
import { ago, exact } from "@/lib/time";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";
import { QueryState } from "@/components/kore/QueryState";

export function OrgAdmin({ onLogin }: { onLogin?: () => void }) {
  const [probe, setProbe] = useState<"loading" | "admin" | "member" | "error">("loading");
  const [err, setErr] = useState<{ kind: ErrorKind; message: string } | null>(null);
  const [usage, setUsage] = useState<OrgUsage | null>(null);
  const [users, setUsers] = useState<OrgUserSummary[]>([]);
  const [members, setMembers] = useState<OrgMemberSummary[]>([]);

  const [section, setSection] = useState<"Accounts" | "Projects" | "Agents" | "Audit">("Accounts");
  const [memberTab, setMemberTab] = useState<"Members" | "Agents">("Members");
  const session = store.session;

  async function load() {
    if (store.demo || !session) {
      setProbe("member");
      api.orgMembers(session ?? "").then(setMembers).catch(() => {});
      return;
    }
    try {
      // role comes from /v1/user/me (admin | member), not from probing /v1/org/usage
      const me = await api.me(session);
      if (me.role === "admin") {
        const [u, us] = await Promise.all([api.orgUsage(session), api.orgUsers(session)]);
        setUsage(u);
        setUsers(us);
        setProbe("admin");
      } else {
        setMembers(await api.orgMembers(session));
        setProbe("member");
      }
      setErr(null);
    } catch (e) {
      const info = errorInfo(e);
      // a failed refresh of an already-loaded screen keeps the data and says so; a failed first load shows the error state
      if (probe === "admin" || probe === "member") toast.error(info.message);
      else {
        setErr(info);
        setProbe("error");
      }
    }
  }
  useEffect(() => {
    load();
  }, []);

  if (probe === "loading") return <QueryState state="loading" message="Loading organization…" className="h-full" />;
  if (probe === "error")
    return (
      <QueryState
        state="error"
        kind={err?.kind}
        message={err?.message}
        onRetry={() => {
          setProbe("loading");
          load();
        }}
        onLogin={onLogin}
        className="h-full"
      />
    );

  return (
    <div className="@container h-full min-w-0 overflow-y-auto">
      <div className="flex min-w-0 flex-col gap-5 p-2">
        <header className="flex min-w-0 items-center gap-3">
          <h1 className="text-ink truncate font-heading text-lg font-semibold tracking-tight">{store.org ?? "Organization"}</h1>
          {probe === "admin" && (
            <span className="inline-flex items-center gap-1.5 rounded-full bg-muted px-2.5 py-1 text-xs text-primary">
              <ShieldCheck size={13} /> Admin
            </span>
          )}
          <IconButton label="Refresh organization" title="Refresh" onClick={load} className="ml-auto shrink-0">
            <RefreshCw size={14} />
          </IconButton>
        </header>

        {probe === "member" ? (
          <>
            <TabPills tabs={[{ label: "Members" as const, count: members.length }, { label: "Agents" as const }]} value={memberTab} onChange={setMemberTab} />
            {memberTab === "Members" ? <MemberDirectory members={members} /> : <AgentsPanel />}
          </>
        ) : (
          <>
            {usage && <UsageCards usage={usage} />}
            <TabPills
              tabs={[{ label: "Accounts" as const, count: users.length }, { label: "Projects" as const, count: usage?.projects.length }, { label: "Agents" as const }, { label: "Audit" as const }]}
              value={section}
              onChange={setSection}
            />
            <div hidden={section !== "Accounts"}>
              <UsersTable users={users} onChanged={load} />
            </div>
            {usage && (
              <div hidden={section !== "Projects"}>
                <ProjectsPanel projects={usage.projects} users={users} onChanged={load} />
              </div>
            )}
            {section === "Agents" && <AgentsPanel />}
            {section === "Audit" && <AuditPanel />}
          </>
        )}
      </div>
    </div>
  );
}

function AccountStatus({ disabled }: { disabled: boolean }) {
  return <StatusDot tone={disabled ? "idle" : "ok"}>{disabled ? "Disabled" : "Enabled"}</StatusDot>;
}

// ---- member (read-only, DU-S4) ----

function MemberDirectory({ members }: { members: OrgMemberSummary[] }) {
  return (
    <section className="min-w-0 rounded-2xl border border-border bg-card p-5">
      <PanelHeader title="Members" />
      <DataTable
        className="mt-5"
        minWidth={420}
        rows={members}
        rowKey={(m) => m.owner_name}
        columns={[
          { header: "Member", cell: (m) => (
            <span className="flex items-center gap-3 font-medium">
              <AgentAvatar seed={m.owner_name} size={26} className="rounded-full" />
              {m.display_name}
            </span>
          ) },
          { header: "Username", mono: true, cell: (m) => <span className="text-muted-foreground">@{m.owner_name}</span> },
          { header: "Role", cell: (m) => <span className="capitalize">{m.role}</span> },
          { header: "Status", cell: (m) => <AccountStatus disabled={m.disabled} /> },
        ]}
      />
      {members.length === 0 && <p className="py-8 text-center text-sm text-muted-foreground">Nobody here yet.</p>}
    </section>
  );
}

// ---- admin: usage (R3, read-only — quota changes are platform-admin) ----

function UsageCards({ usage }: { usage: OrgUsage }) {
  const tiles = [
    { label: "Agents", value: usage.agents, detail: `of ${usage.max_agents} quota`, quota: true },
    { label: "Humans", value: usage.humans, detail: "Online identities" },
    { label: "Accounts", value: usage.users, detail: "Organization logins" },
    { label: "Messages", value: usage.messages_30d, detail: "Last 30 days" },
  ];
  return (
    <div className="grid grid-cols-2 gap-3 @2xl:grid-cols-4">
      {tiles.map((tile) => (
        <div key={tile.label} className="min-w-0 rounded-2xl border border-border bg-card p-4">
          <Eyebrow>{tile.label}</Eyebrow>
          <p className="mt-2 font-heading text-3xl font-semibold tracking-tight tabular-nums">{tile.value.toLocaleString("en")}</p>
          <p className="mt-0.5 text-xs text-muted-foreground">{tile.detail}</p>
          {tile.quota && <div className="mt-3"><Meter value={usage.agents} max={usage.max_agents} /></div>}
        </div>
      ))}
    </div>
  );
}

// ---- admin: accounts (R1 — role / disable / password reset, each through a dialog) ----

type Action =
  | { kind: "role"; user: OrgUserSummary; to: "admin" | "member" }
  | { kind: "disable"; user: OrgUserSummary; disabled: boolean }
  | { kind: "password"; user: OrgUserSummary };

const MIN_PASSWORD = 10;

function ActionDialog({ action, onClose, onChanged }: { action: Action | null; onClose: () => void; onChanged: () => void }) {
  const session = store.session!;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [password, setPassword] = useState("");
  const [done, setDone] = useState<string | null>(null);
  // keep the last action while the close animation runs
  const last = useRef<Action | null>(null);
  if (action) last.current = action;
  const a = action ?? last.current;

  useEffect(() => {
    setBusy(false);
    setError(null);
    setPassword("");
    setDone(null);
  }, [action]);

  if (!a) return null;
  const name = a.user.display_name;
  const who = `${name} (${a.user.email})`;
  const tooShort = a.kind === "password" && [...password].length < MIN_PASSWORD;

  const copy =
    a.kind === "role"
      ? {
          title: a.to === "admin" ? `Make ${name} an admin?` : `Make ${name} a member?`,
          body:
            a.to === "admin"
              ? `${who} will be able to manage accounts, projects and passwords for the whole organization.`
              : `${who} will lose admin access: no more account, project or password management.`,
          cta: a.to === "admin" ? "Make admin" : "Make member",
        }
      : a.kind === "disable"
        ? {
            title: a.disabled ? `Disable ${name}?` : `Enable ${name}?`,
            body: a.disabled
              ? `${who} will no longer be able to sign in. Their agents and history stay.`
              : `${who} will be able to sign in again.`,
            cta: a.disabled ? "Disable account" : "Enable account",
          }
        : { title: `Reset password for ${name}`, body: `Set a new password for ${who}. They will need it to sign in.`, cta: "Reset password" };

  async function submit() {
    if (!a) return;
    setBusy(true);
    setError(null);
    try {
      if (a.kind === "role") await api.updateOrgUser(session, a.user.id, { role: a.to });
      else if (a.kind === "disable") await api.updateOrgUser(session, a.user.id, { disabled: a.disabled });
      else await api.updateOrgUser(session, a.user.id, { password });
      onChanged();
      if (a.kind === "password") {
        setPassword("");
        setDone(`Password updated for ${name}.`);
      } else onClose();
    } catch (e) {
      // 409 (last admin) and friends: show the server's own message
      setError(errorInfo(e).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={!!action} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>{done ? "Password reset" : copy.title}</DialogTitle>
          <DialogDescription>{done ?? copy.body}</DialogDescription>
        </DialogHeader>
        {!done && a.kind === "password" && (
          <div className="space-y-1.5">
            <label htmlFor="org-new-password" className="text-xs font-medium">New password</label>
            <input
              id="org-new-password"
              type="password"
              autoComplete="new-password"
              autoFocus
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && !tooShort && !busy && submit()}
              aria-invalid={password.length > 0 && tooShort}
              aria-describedby="org-new-password-hint"
              className="w-full rounded-xl border border-input bg-card px-4 py-2.5 text-sm outline-none focus:border-primary"
            />
            <p id="org-new-password-hint" className={cn("text-xs", password.length > 0 && tooShort ? "text-destructive" : "text-muted-foreground")}>
              At least {MIN_PASSWORD} characters ({[...password].length}/{MIN_PASSWORD}).
            </p>
          </div>
        )}
        {error && <p role="alert" className="rounded-xl border border-border bg-muted px-3 py-2 text-sm text-destructive">{error}</p>}
        <DialogFooter>
          {done ? (
            <Button onClick={onClose}>Close</Button>
          ) : (
            <>
              <Button variant="outline" onClick={onClose} disabled={busy}>Cancel</Button>
              <Button onClick={submit} disabled={busy || tooShort}>{busy ? "Working…" : copy.cta}</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function UsersTable({ users, onChanged }: { users: OrgUserSummary[]; onChanged: () => void }) {
  const [action, setAction] = useState<Action | null>(null);

  return (
    <section aria-label="Accounts" className="min-w-0 rounded-2xl border border-border bg-card p-5">
      <DataTable
        rows={users}
        minWidth={620}
        rowKey={(u) => u.id}
        columns={[
          { header: "Account", cell: (u) => (
            <div className="flex items-center gap-3">
              <AgentAvatar seed={u.display_name} size={26} className="rounded-full" />
              <div>
                <p className="font-medium">{u.display_name}</p>
                <p className="break-all text-xs text-muted-foreground">{u.email}</p>
              </div>
            </div>
          ) },
          { header: "Role", cell: (u) => (
            <Button
              variant={u.role === "admin" ? "primary" : "outline"}
              size="sm"
              className="px-3 py-1 capitalize"
              title="Change role"
              aria-label={`Change role of ${u.display_name} (now ${u.role})`}
              onClick={() => setAction({ kind: "role", user: u, to: u.role === "admin" ? "member" : "admin" })}
            >
              {u.role}
            </Button>
          ) },
          { header: "Status", cell: (u) => <AccountStatus disabled={u.disabled} /> },
          { header: "Actions", cell: (u) => (
            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" className="px-3 py-1" onClick={() => setAction({ kind: "disable", user: u, disabled: !u.disabled })}>
                {u.disabled ? "Enable" : "Disable"}
              </Button>
              <IconButton label={`Reset password for ${u.display_name}`} title="Reset password" onClick={() => setAction({ kind: "password", user: u })}>
                <KeyRound size={14} />
              </IconButton>
            </div>
          ) },
        ]}
      />
      {users.length === 0 && <p className="py-8 text-center text-sm text-muted-foreground">No accounts yet.</p>}
      <ActionDialog action={action} onClose={() => setAction(null)} onChanged={onChanged} />
    </section>
  );
}

// ---- agents (read-only; admin = whole org, member = their projects — the server scopes it) ----

function AgentsPanel() {
  const [rows, setRows] = useState<OrgAgent[] | null>(null);
  const [err, setErr] = useState<{ kind: ErrorKind; message: string } | null>(null);
  const load = useCallback(() => {
    setErr(null);
    setRows(null);
    api.orgAgents(store.session ?? "").then(setRows).catch((e) => setErr(errorInfo(e)));
  }, []);
  useEffect(load, [load]);

  if (err) return <QueryState state="error" kind={err.kind} message={err.message} onRetry={load} />;
  if (!rows) return <QueryState state="loading" message="Loading agents…" />;
  if (rows.length === 0) return <QueryState state="empty" message="No agents in your projects yet." />;
  return (
    <section aria-label="Agents" className="min-w-0 rounded-2xl border border-border bg-card p-5">
      <DataTable
        rows={rows}
        minWidth={640}
        rowKey={(a) => a.id}
        columns={[
          { header: "Agent", cell: (a) => <span className="font-medium">{a.name}</span> },
          { header: "Project", mono: true, cell: (a) => a.project },
          { header: "Role", cell: (a) => <span className="text-muted-foreground">{a.role ?? "—"}</span> },
          { header: "Status", cell: (a) => <StatusDot tone={a.status === "active" ? "ok" : "idle"}><span className="capitalize">{a.status}</span></StatusDot> },
          { header: "Last seen", nowrap: true, cell: (a) => <span title={exact(a.last_seen)}>{ago(a.last_seen)}</span> },
          { header: "Messages sent", mono: true, cell: (a) => a.messages_sent.toLocaleString("en") },
        ]}
      />
    </section>
  );
}

// ---- audit (admin) ----

const AUDIT_PAGE = 50;

function AuditPanel() {
  const [rows, setRows] = useState<AuditEntry[] | null>(null);
  const [more, setMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [err, setErr] = useState<{ kind: ErrorKind; message: string } | null>(null);

  const fetchPage = useCallback(async (before?: number) => {
    const page = await api.orgAudit(store.session ?? "", AUDIT_PAGE, before);
    setMore(page.length === AUDIT_PAGE);
    return page;
  }, []);
  const load = useCallback(() => {
    setErr(null);
    setRows(null);
    fetchPage().then(setRows).catch((e) => setErr(errorInfo(e)));
  }, [fetchPage]);
  useEffect(load, [load]);

  async function loadMore() {
    if (!rows?.length) return;
    setLoadingMore(true);
    try {
      const page = await fetchPage(rows[rows.length - 1].id);
      setRows((r) => [...(r ?? []), ...page]);
    } catch (e) {
      toast.error(errorInfo(e).message);
    } finally {
      setLoadingMore(false);
    }
  }

  if (err) return <QueryState state="error" kind={err.kind} message={err.message} onRetry={load} />;
  if (!rows) return <QueryState state="loading" message="Loading audit log…" />;
  if (rows.length === 0) return <QueryState state="empty" message="Nothing in the audit log yet." />;
  return (
    <section aria-label="Audit log" className="min-w-0 rounded-2xl border border-border bg-card p-5">
      <DataTable
        rows={rows}
        minWidth={620}
        rowKey={(e) => String(e.id)}
        columns={[
          { header: "Who", nowrap: true, cell: (e) => e.actor_label },
          { header: "What", mono: true, nowrap: true, cell: (e) => e.action },
          { header: "On", cell: (e) => <span className="break-all text-muted-foreground">{e.target ?? "—"}</span> },
          { header: "When", nowrap: true, cell: (e) => <span title={exact(e.created_at)}>{ago(e.created_at)}</span> },
        ]}
      />
      {more && (
        <div className="mt-4 flex justify-center">
          <Button variant="outline" size="sm" onClick={loadMore} disabled={loadingMore}>
            {loadingMore ? "Loading…" : "Load more"}
          </Button>
        </div>
      )}
    </section>
  );
}

// ---- admin: projects + membership (DU-S1 endpoints) ----

function ProjectsPanel({ projects, users, onChanged }: { projects: { name: string; instances: number }[]; users: OrgUserSummary[]; onChanged: () => void }) {
  const session = store.session!;
  const [newName, setNewName] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const [members, setMembers] = useState<ProjectMemberSummary[]>([]);

  async function create() {
    const name = newName.trim();
    if (!name) return;
    try {
      await api.createOrgProject(session, name);
      setNewName("");
      toast.success(`Project '${name}' created — you're its first member`);
      onChanged();
    } catch (e) {
      toast.error(String(e));
    }
  }
  async function toggle(name: string) {
    if (open === name) return setOpen(null);
    setOpen(name);
    setMembers([]);
    try {
      setMembers(await api.orgProjectMembers(session, name));
    } catch (e) {
      toast.error(String(e));
    }
  }
  async function refreshMembers(name: string) {
    setMembers(await api.orgProjectMembers(session, name).catch(() => []));
  }
  const assignable = useMemo(
    () => users.filter((u) => !u.disabled && !members.some((m) => m.user_id === u.id)),
    [users, members],
  );

  return (
    <section aria-label="Projects" className="min-w-0 space-y-3">
      <div className="flex items-center gap-2">
        <input
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
          aria-label="New project name"
          placeholder="New project name"
          className="min-w-0 flex-1 rounded-xl border border-input bg-card px-4 py-2.5 text-sm outline-none placeholder:text-muted-foreground focus:border-primary"
        />
        <Button className="rounded-xl" onClick={create} disabled={!newName.trim()}>
          <FolderPlus /> Create
        </Button>
      </div>
      {projects.map((p) => (
        <div key={p.name} className="overflow-hidden rounded-2xl border border-border bg-card">
          <button
            type="button"
            aria-expanded={open === p.name}
            onClick={() => toggle(p.name)}
            className="flex w-full items-center gap-3 p-4 text-left transition-colors hover:bg-muted focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary"
          >
            <span className="truncate font-mono text-sm font-medium">{p.name}</span>
            <span className="shrink-0 rounded-lg border border-border px-2 py-0.5 text-xs text-muted-foreground">
              {p.instances} {p.instances === 1 ? "instance" : "instances"}
            </span>
            <ChevronDown size={15} className={cn("ml-auto shrink-0 text-muted-foreground transition-transform", open === p.name && "rotate-180")} />
          </button>
          {open === p.name && (
            <div className="space-y-4 border-t border-border/70 p-4">
              <div>
                <Eyebrow>Members</Eyebrow>
                <div className="mt-2 flex flex-wrap gap-2">
                  {members.map((m) => (
                    <div key={m.user_id} className="flex max-w-full items-center gap-2 rounded-full bg-muted py-1 pl-1 pr-1.5">
                      <AgentAvatar seed={m.owner_name} size={22} className="rounded-full" />
                      <span className="truncate text-sm">{m.display_name}</span>
                      <span className="truncate font-mono text-xs text-muted-foreground">@{m.owner_name}</span>
                      <IconButton
                        label={`Remove ${m.display_name} from project`}
                        title="Remove from project"
                        className="size-6 shrink-0"
                        onClick={async () => {
                          try {
                            await api.removeOrgProjectMember(session, p.name, m.user_id);
                            refreshMembers(p.name);
                          } catch (e) {
                            toast.error(String(e));
                          }
                        }}
                      >
                        <UserMinus size={12} />
                      </IconButton>
                    </div>
                  ))}
                </div>
              </div>
              {assignable.length > 0 && (
                <div>
                  <Eyebrow>Add members</Eyebrow>
                  <div className="mt-2 flex flex-wrap gap-2">
                    {assignable.map((u) => (
                      <Button
                        key={u.id}
                        variant="outline"
                        size="sm"
                        className="gap-1.5 px-3 py-1.5"
                        onClick={async () => {
                          try {
                            await api.addOrgProjectMember(session, p.name, u.id);
                            refreshMembers(p.name);
                          } catch (e) {
                            toast.error(String(e));
                          }
                        }}
                      >
                        <UserPlus size={12} /> {u.display_name}
                      </Button>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      ))}
      {projects.length === 0 && <p className="py-8 text-center text-sm text-muted-foreground">No projects yet. Create the first one above.</p>}
    </section>
  );
}
