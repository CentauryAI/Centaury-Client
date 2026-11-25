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
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";
import { ChevronDown, Loader2 } from "lucide-react";
import Button from "@/components/centaury/primitives/Button";

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
  const [advanced, setAdvanced] = useState(false);

  useEffect(() => {
    if (!open) return;
    setProj(project);
    setOwner("");
    setProjQuery("");
    setOwnerQuery("");
    setAdvanced(false);
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

  // detect_tools answered and the chosen tool is not on this computer → nothing to launch
  const toolMissing = tools.length > 0 && !tools.find((t) => t.name === tool)?.found;

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
      <DialogContent className="sm:max-w-[420px] rounded-2xl">
        <DialogHeader>
          <DialogTitle className="font-heading text-base font-semibold">
            Summon agents
          </DialogTitle>
          <DialogDescription>
            Starts agents on this computer with centaury.
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
                className="h-9 w-full rounded-xl border border-input bg-transparent px-2 text-sm focus:border-primary focus:outline-none"
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
              <Input id="count" type="number" min={1} max={9} value={count} onChange={(e) => setCount(Number(e.target.value) || 1)} className="rounded-xl focus:border-primary" />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="proj">Project</Label>
            <Input
              id="proj"
              value={projQuery}
              onChange={(e) => setProjQuery(e.target.value)}
              placeholder="type to filter projects…"
              className="h-8 rounded-xl focus:border-primary"
            />
            <div className="max-h-24 overflow-y-auto rounded-xl border border-border">
              {projOptions.map((p) => (
                <button
                  type="button"
                  key={p.name}
                  onClick={() => setProj(p.name)}
                  className={
                    "flex w-full items-center justify-between px-2 py-1 text-left text-sm transition-colors hover:bg-muted " +
                    (proj === p.name ? "bg-muted font-medium" : "")
                  }
                >
                  <span>{p.name}</span>
                  <span className="text-[10px] text-muted-foreground">{p.instance_count} instances</span>
                </button>
              ))}
              {projOptions.length === 0 && <p className="px-2 py-1 text-xs text-muted-foreground">no matching project</p>}
            </div>
          </div>
          <div>
            <button
              type="button"
              aria-expanded={advanced}
              aria-controls="summon-advanced"
              onClick={() => setAdvanced((v) => !v)}
              className="flex items-center gap-1.5 text-sm font-medium text-muted-foreground transition-colors hover:text-foreground"
            >
              <ChevronDown className={"h-4 w-4 transition-transform " + (advanced ? "" : "-rotate-90")} /> Advanced
            </button>
            {advanced && (
              <div id="summon-advanced" className="mt-3 space-y-3">
          <div className="space-y-1.5">
            <Label htmlFor="owner">Who controls it</Label>
            <div className="flex flex-wrap gap-1">
              <button
                type="button"
                onClick={() => setOwner("")}
                className={
                  "rounded-full border px-2.5 py-0.5 text-xs transition-colors " +
                  (owner === "" ? "border-primary bg-foreground text-background" : "border-border hover:bg-muted")
                }
              >
                you ({store.name})
              </button>
              {owner !== "" && (
                <span className="rounded-full border border-primary bg-foreground px-2.5 py-0.5 text-xs text-background">
                  {owner}
                </span>
              )}
            </div>
            <Input
              id="owner"
              value={ownerQuery}
              onChange={(e) => setOwnerQuery(e.target.value)}
              placeholder="search org members to delegate…"
              className="h-8 rounded-xl focus:border-primary"
            />
            {ownerQuery && (
              <div className="max-h-24 overflow-y-auto rounded-xl border border-border">
                {ownerOptions.map((m) => (
                  <button
                    type="button"
                    key={m.owner_name}
                    onClick={() => {
                      setOwner(m.owner_name);
                      setOwnerQuery("");
                    }}
                    className="flex w-full items-center justify-between px-2 py-1 text-left text-sm transition-colors hover:bg-muted"
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
              <Input id="tag" value={tag} onChange={(e) => setTag(e.target.value)} placeholder="crew" className="rounded-xl focus:border-primary" />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="dir">Start in folder</Label>
              <Input id="dir" value={dir} onChange={(e) => setDir(e.target.value)} placeholder="Your home folder" className="rounded-xl focus:border-primary" />
            </div>
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="extra">Extra options for the tool (optional)</Label>
            <Input id="extra" value={extra} onChange={(e) => setExtra(e.target.value)} placeholder="e.g. --model opus" className="rounded-xl focus:border-primary" />
          </div>
          <label className="flex cursor-pointer items-center gap-2 text-sm">
            <input type="checkbox" checked={background} onChange={(e) => setBackground(e.target.checked)} className="h-4 w-4 accent-primary" />
            Run in the background (no terminal window, keeps listening)
          </label>
              </div>
            )}
          </div>
          {toolMissing ? (
          <p data-testid="summon-summary" className="rounded-xl bg-muted px-3 py-2 text-xs text-muted-foreground">
            None of the supported tools is installed on this computer, so there is nothing to start.
          </p>
          ) : (
          <p data-testid="summon-summary" className="rounded-xl bg-muted px-3 py-2 text-xs text-muted-foreground">
            Will start {count} {tool} {count > 1 ? "agents" : "agent"} in <span className="font-medium text-foreground">{proj || "…"}</span>
            {owner ? <>, controlled by @{owner}</> : null}
            {tag.trim() ? <>, tagged {tag.trim()}</> : null}
            {dir.trim() ? <>, in {dir.trim()}</> : null}, {background ? "in the background" : "in a terminal window"}.
          </p>
          )}
          <Button type="submit" className="w-full" variant="primary" size="md" disabled={busy || !proj.trim() || toolMissing}>
            {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            Summon {count > 1 ? `${count} agents` : "agent"}
          </Button>
        </form>
      </DialogContent>
    </Dialog>
  );
}
