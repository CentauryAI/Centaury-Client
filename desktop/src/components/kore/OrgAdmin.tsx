// OrgAdmin (DU-D10, R1/R3) — org administration in the app. Role probed like
// org.html: GET /v1/org/usage 200 = admin (full controls), 403 = member
// (read-only directory, DU-S4). All writes ride the SESSION token; the server
// is the authority — this screen is a thin remote control.
import { useEffect, useMemo, useState } from "react";
import { Building2, FolderPlus, KeyRound, RefreshCw, ShieldCheck, UserMinus, UserPlus } from "lucide-react";
import { api, store, type OrgMemberSummary, type OrgUsage, type OrgUserSummary, type ProjectMemberSummary } from "@/lib/api";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { toast } from "sonner";

export function OrgAdmin() {
  const [probe, setProbe] = useState<"loading" | "admin" | "member">("loading");
  const [usage, setUsage] = useState<OrgUsage | null>(null);
  const [users, setUsers] = useState<OrgUserSummary[]>([]);
  const [members, setMembers] = useState<OrgMemberSummary[]>([]);

  const session = store.session;

  async function load() {
    if (store.demo || !session) {
      setProbe("member");
      api.orgMembers(session ?? "").then(setMembers).catch(() => {});
      return;
    }
    try {
      const u = await api.orgUsage(session);
      setUsage(u);
      setProbe("admin");
      api.orgUsers(session).then(setUsers).catch((e) => toast.error(String(e)));
    } catch {
      // 403 (or expired session) = not an admin here — show the directory
      setProbe("member");
      api.orgMembers(session).then(setMembers).catch((e) => toast.error(String(e)));
    }
  }
  useEffect(() => {
    load();
  }, []);

  if (probe === "loading") return <div className="p-8 text-sm text-muted-foreground">Loading org…</div>;

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="mx-auto flex max-w-3xl flex-col gap-6">
        <div className="flex items-center gap-2">
          <Building2 className="h-5 w-5" />
          <h1 className="text-lg font-semibold">{store.org ?? "Organization"}</h1>
          {probe === "admin" && (
            <Badge variant="secondary" className="gap-1">
              <ShieldCheck className="h-3 w-3" /> admin
            </Badge>
          )}
          <Button variant="ghost" size="icon" className="ml-auto h-8 w-8" onClick={load} title="Refresh">
            <RefreshCw className="h-4 w-4" />
          </Button>
        </div>

        {probe === "member" ? (
          <MemberDirectory members={members} />
        ) : (
          <>
            {usage && <UsageCards usage={usage} />}
            <UsersTable users={users} onChanged={load} />
            {usage && <ProjectsPanel projects={usage.projects} users={users} onChanged={load} />}
          </>
        )}
      </div>
    </div>
  );
}

// ---- member (read-only, DU-S4) ----

function MemberDirectory({ members }: { members: OrgMemberSummary[] }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">Members</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1.5">
        {members.map((m) => (
          <div key={m.owner_name} className="flex items-center gap-2.5 rounded px-1 py-1">
            <AgentAvatar seed={m.owner_name} size={26} className="rounded-full" />
            <span className="text-sm font-medium">{m.display_name}</span>
            <span className="font-mono text-xs text-muted-foreground">@{m.owner_name}</span>
            {m.role === "admin" && <Badge variant="outline" className="text-[10px]">admin</Badge>}
            {m.disabled && <Badge variant="destructive" className="text-[10px]">disabled</Badge>}
          </div>
        ))}
        {members.length === 0 && <p className="text-xs text-muted-foreground">Nobody here yet.</p>}
      </CardContent>
    </Card>
  );
}

// ---- admin: usage (R3, read-only — quota changes are platform-admin) ----

function UsageCards({ usage }: { usage: OrgUsage }) {
  const cells: [string, string][] = [
    ["Agents", `${usage.agents} / ${usage.max_agents}`],
    ["Humans", String(usage.humans)],
    ["Accounts", String(usage.users)],
    ["Msgs · 30d", String(usage.messages_30d)],
  ];
  return (
    <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
      {cells.map(([label, value]) => (
        <Card key={label}>
          <CardContent className="p-4">
            <div className="text-[11px] uppercase tracking-wider text-muted-foreground">{label}</div>
            <div className="mt-1 text-xl font-semibold">{value}</div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

// ---- admin: accounts (R1 — role / disable / password reset) ----

function UsersTable({ users, onChanged }: { users: OrgUserSummary[]; onChanged: () => void }) {
  const session = store.session!;

  async function patch(u: OrgUserSummary, body: { role?: string; disabled?: boolean; password?: string }, done: string) {
    try {
      await api.updateOrgUser(session, u.id, body);
      toast.success(done);
      onChanged();
    } catch (e) {
      toast.error(String(e));
    }
  }
  function resetPassword(u: OrgUserSummary) {
    const pw = window.prompt(`New password for ${u.email} (min 8 chars):`);
    if (!pw) return;
    patch(u, { password: pw }, `Password reset for ${u.display_name}`);
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">Accounts</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        {users.map((u) => (
          <div key={u.id} className="flex items-center gap-2.5 rounded px-1 py-1.5">
            <AgentAvatar seed={u.display_name} size={26} className="rounded-full" />
            <div className="min-w-0 flex-1 leading-tight">
              <div className="flex items-center gap-1.5">
                <span className="truncate text-sm font-medium">{u.display_name}</span>
                {u.disabled && <Badge variant="destructive" className="text-[10px]">disabled</Badge>}
              </div>
              <div className="truncate text-xs text-muted-foreground">{u.email}</div>
            </div>
            <Button
              variant={u.role === "admin" ? "secondary" : "outline"}
              size="sm"
              className="h-7 px-2 text-xs"
              title="Toggle role"
              onClick={() => patch(u, { role: u.role === "admin" ? "member" : "admin" }, `${u.display_name} is now ${u.role === "admin" ? "member" : "admin"}`)}
            >
              {u.role}
            </Button>
            <Button variant="outline" size="sm" className="h-7 px-2 text-xs" onClick={() => patch(u, { disabled: !u.disabled }, u.disabled ? "Enabled" : "Disabled")}>
              {u.disabled ? "enable" : "disable"}
            </Button>
            <Button variant="ghost" size="icon" className="h-7 w-7" title="Reset password" onClick={() => resetPassword(u)}>
              <KeyRound className="h-3.5 w-3.5" />
            </Button>
          </div>
        ))}
      </CardContent>
    </Card>
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
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm">Projects</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1">
        <div className="mb-2 flex gap-2">
          <Input value={newName} onChange={(e) => setNewName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && create()} placeholder="new project name" className="h-8 text-sm" />
          <Button size="sm" className="h-8" onClick={create} disabled={!newName.trim()}>
            <FolderPlus className="mr-1.5 h-3.5 w-3.5" /> Create
          </Button>
        </div>
        {projects.map((p) => (
          <div key={p.name} className="rounded border">
            <button onClick={() => toggle(p.name)} className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-accent/50">
              <span className="flex-1 truncate text-sm font-medium">{p.name}</span>
              <span className="text-xs text-muted-foreground">{p.instances} instances</span>
            </button>
            {open === p.name && (
              <div className="border-t px-3 py-2">
                {members.map((m) => (
                  <div key={m.user_id} className="flex items-center gap-2 py-1">
                    <AgentAvatar seed={m.owner_name} size={22} className="rounded-full" />
                    <span className="flex-1 truncate text-sm">{m.display_name}</span>
                    <span className="font-mono text-xs text-muted-foreground">@{m.owner_name}</span>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-6 w-6"
                      title="Remove from project"
                      onClick={async () => {
                        try {
                          await api.removeOrgProjectMember(session, p.name, m.user_id);
                          refreshMembers(p.name);
                        } catch (e) {
                          toast.error(String(e));
                        }
                      }}
                    >
                      <UserMinus className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))}
                {assignable.length > 0 && (
                  <div className="mt-1.5 flex flex-wrap gap-1.5 border-t pt-2">
                    {assignable.map((u) => (
                      <Button
                        key={u.id}
                        variant="outline"
                        size="sm"
                        className="h-6 gap-1 px-2 text-xs"
                        onClick={async () => {
                          try {
                            await api.addOrgProjectMember(session, p.name, u.id);
                            refreshMembers(p.name);
                          } catch (e) {
                            toast.error(String(e));
                          }
                        }}
                      >
                        <UserPlus className="h-3 w-3" /> {u.display_name}
                      </Button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        ))}
        {projects.length === 0 && <p className="text-xs text-muted-foreground">No projects yet — create the first one above.</p>}
      </CardContent>
    </Card>
  );
}
