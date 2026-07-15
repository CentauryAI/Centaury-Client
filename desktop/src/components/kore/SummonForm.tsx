// SummonForm — launch agents into a project bubble (DU-D4, per
// HANDOFF-2026-07-07 + owner directive: project is a SELECT over the org's
// existing projects (searchable, no free text — admin-created projects are
// DU-S1's fence); owner defaults to YOU with a searchable picker over the org
// directory (DU-S4) to DELEGATE the agent to another account (Q1 — the server
// validates the delegate is a real account). Spawning shells to
// `centaury launch` through the Tauri `spawn_launch` (DU-D1 — reuse the CLI).
import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { api, store, type OrgMemberSummary, type ProjectSummary } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";
import { Loader2, Rocket } from "lucide-react";

type ToolHit = { name: string; found: boolean };

export function SummonForm({ project, open, onClose, onLaunched }: {
  project: string;
  open: boolean;
  onClose: () => void;
  onLaunched: () => void;
}) {
  const [tools, setTools] = useState<ToolHit[]>([]);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [members, setMembers] = useState<OrgMemberSummary[]>([]);
  const [tool, setTool] = useState("claude");
  const [proj, setProj] = useState(project);
  const [projQuery, setProjQuery] = useState("");
  const [owner, setOwner] = useState(""); // "" = you (server default)
  const [ownerQuery, setOwnerQuery] = useState("");
  const [count, setCount] = useState(1);
  const [tag, setTag] = useState("");
  const [background, setBackground] = useState(true);
  const [dir, setDir] = useState("");
  const [extra, setExtra] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setProj(project);
    setOwner("");
    setProjQuery("");
    setOwnerQuery("");
    invoke<ToolHit[]>("detect_tools")
      .then((t) => {
        setTools(t);
        const first = t.find((x) => x.found);
        if (first && !t.find((x) => x.name === tool)?.found) setTool(first.name);
      })
      .catch(() => setTools([]));
    if (store.session) {
      api.userProjects(store.session).then(setProjects).catch(() => {});
      api.orgMembers(store.session).then((m) => setMembers(m.filter((x) => !x.disabled))).catch(() => {});
    }
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  const projOptions = useMemo(
    () => projects.filter((p) => p.name.toLowerCase().includes(projQuery.toLowerCase())),
    [projects, projQuery]
  );
  const ownerOptions = useMemo(
    () =>
      members.filter(
        (m) =>
          m.owner_name !== store.name &&
          (m.owner_name.toLowerCase().includes(ownerQuery.toLowerCase()) ||
            m.display_name.toLowerCase().includes(ownerQuery.toLowerCase()))
      ),
    [members, ownerQuery]
  );

  async function launch(e: React.FormEvent) {
    e.preventDefault();
    if (store.demo) {
      toast.info("Demo mode — launching needs a real server + centaury.");
      return;
    }
    setBusy(true);
    try {
      const out = await invoke<string>("spawn_launch", {
        opts: {
          server: store.server,
          token: store.token,
          project: proj.trim(),
          tool,
          count,
          tag: tag.trim() || null,
          owner: owner || null, // null = you; a name DELEGATES (server-validated, DU-S2)
          headless: background,
          dir: dir.trim() || null,
          args: extra.trim() ? extra.trim().split(/\s+/) : [],
        },
      });
      toast.success(out.trim().split("\n").slice(-1)[0] || "launched");
      onLaunched();
      onClose();
    } catch (err) {
      toast.error(String(err)); // centaury's error verbatim (plan rule)
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={(v) => !v && onClose()}>
      <DialogContent className="sm:max-w-[420px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Rocket className="h-4 w-4" /> Summon agents
          </DialogTitle>
          <DialogDescription>
            Launches via centaury on this machine. The agent's owner is you (your account).
          </DialogDescription>
        </DialogHeader>
        <form onSubmit={launch} className="space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="tool">Tool</Label>
              <select
                id="tool"
                value={tool}
                onChange={(e) => setTool(e.target.value)}
                className="h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm"
              >
                {tools.map((t) => (
                  <option key={t.name} value={t.name} disabled={!t.found}>
                    {t.name}{t.found ? "" : " (not installed)"}
                  </option>
                ))}
              </select>
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="count">Count</Label>
              <Input id="count" type="number" min={1} max={9} value={count} onChange={(e) => setCount(Number(e.target.value) || 1)} />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="proj">Project</Label>
            <Input
              id="proj"
              value={projQuery}
              onChange={(e) => setProjQuery(e.target.value)}
              placeholder="type to filter projects…"
              className="h-8"
            />
            <div className="max-h-24 overflow-y-auto rounded-md border">
              {projOptions.map((p) => (
                <button
                  type="button"
                  key={p.name}
                  onClick={() => setProj(p.name)}
                  className={
                    "flex w-full items-center justify-between px-2 py-1 text-left text-sm hover:bg-accent " +
                    (proj === p.name ? "bg-accent font-medium" : "")
                  }
                >
                  <span>{p.name}</span>
                  <span className="text-[10px] text-muted-foreground">{p.instance_count} instances</span>
                </button>
              ))}
              {projOptions.length === 0 && <p className="px-2 py-1 text-xs text-muted-foreground">no matching project</p>}
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="owner">Owner</Label>
            <div className="flex flex-wrap gap-1">
              <button
                type="button"
                onClick={() => setOwner("")}
                className={
                  "rounded-full border px-2.5 py-0.5 text-xs " +
                  (owner === "" ? "border-primary bg-primary text-primary-foreground" : "hover:bg-accent")
                }
              >
                you ({store.name})
              </button>
              {owner !== "" && (
                <span className="rounded-full border border-primary bg-primary px-2.5 py-0.5 text-xs text-primary-foreground">
                  {owner}
                </span>
              )}
            </div>
            <Input
              id="owner"
              value={ownerQuery}
              onChange={(e) => setOwnerQuery(e.target.value)}
              placeholder="search org members to delegate…"
              className="h-8"
            />
            {ownerQuery && (
              <div className="max-h-24 overflow-y-auto rounded-md border">
                {ownerOptions.map((m) => (
                  <button
                    type="button"
                    key={m.owner_name}
                    onClick={() => {
                      setOwner(m.owner_name);
                      setOwnerQuery("");
                    }}
                    className="flex w-full items-center justify-between px-2 py-1 text-left text-sm hover:bg-accent"
                  >
                    <span>{m.display_name}</span>
                    <span className="text-[10px] text-muted-foreground">@{m.owner_name} · {m.role}</span>
                  </button>
                ))}
                {ownerOptions.length === 0 && <p className="px-2 py-1 text-xs text-muted-foreground">no matching member</p>}
              </div>
            )}
            {owner !== "" && (
              <p className="text-[11px] text-muted-foreground">
                Delegating: @{owner} will command this agent, not you.
              </p>
            )}
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="tag">Tag (optional)</Label>
              <Input id="tag" value={tag} onChange={(e) => setTag(e.target.value)} placeholder="crew" />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="dir">Directory</Label>
              <Input id="dir" value={dir} onChange={(e) => setDir(e.target.value)} placeholder="~ (default)" />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="extra">Extra tool args</Label>
            <Input id="extra" value={extra} onChange={(e) => setExtra(e.target.value)} placeholder="passed after --" />
          </div>
          <label className="flex cursor-pointer items-center gap-2 text-sm">
            <input type="checkbox" checked={background} onChange={(e) => setBackground(e.target.checked)} className="h-4 w-4 accent-primary" />
            Launch in background (no terminal window, stays listening)
          </label>
          <Button type="submit" className="w-full" disabled={busy || !proj.trim()}>
            {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Summon {count > 1 ? `${count} agents` : "agent"}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
