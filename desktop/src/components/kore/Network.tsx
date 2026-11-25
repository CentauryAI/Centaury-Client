// Network — the collaboration graph, its own page (moved out of the workspace).
// Two levels: (1) Organization = projects orbiting the Centaury core (real data:
// name + instance_count); click a project to drill in. (2) Project = its roster
// as a graph — humans (owners) ringed, their agents orbiting them, all wired to
// the core. Built on React Flow (@xyflow/react): pan / zoom / drag / minimap /
// select for free. Node positions are computed here (radial). Every view also has a
// keyboard-accessible table with the same rows (Graph | List switch).
//
// Roster source of a drilled project: that project's own stored token
// (/v1/instances: owners, tools) when it was opened before; otherwise the org
// directory GET /v1/org/agents (names, role, status — no owner/tool). No live
// traffic: the server gives a human no reliable agent↔agent stream.
import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  Handle,
  Position,
  useNodesState,
  useEdgesState,
  useNodesInitialized,
  useReactFlow,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ArrowLeft, ArrowUpRight, FolderGit2 } from "lucide-react";
import { Button, DataTable, IconButton, PanelHeader, StatusDot, TabPills, cn } from "@/components/centaury";
import { api, errorInfo, store, type ErrorKind, type InstanceSummary, type ProjectSummary } from "@/lib/api";
import { mockOrg, mockRoster } from "@/lib/mock";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";
import { QueryState } from "@/components/kore/QueryState";

const EDGE_STROKE = "color-mix(in srgb, var(--foreground) 22%, transparent)";
const EDGE_STYLE = { stroke: EDGE_STROKE, strokeWidth: 1.25, strokeDasharray: "4 4" } as const;
const AGENT_EDGE_STYLE = { stroke: EDGE_STROKE, strokeWidth: 1.25, strokeDasharray: "3 3" } as const;

const FLOW_VARS = {
  "--xy-background-color": "transparent",
  "--xy-edge-stroke": EDGE_STROKE,
  "--xy-edge-stroke-width": "1.25",
  "--xy-edge-stroke-selected": "var(--primary)",
  "--xy-controls-button-background-color": "var(--card)",
  "--xy-controls-button-background-color-hover": "var(--muted)",
  "--xy-controls-button-color": "var(--muted-foreground)",
  "--xy-controls-button-color-hover": "var(--foreground)",
  "--xy-controls-button-border-color": "var(--border)",
  "--xy-controls-box-shadow": "0 1px 2px 0 color-mix(in srgb, var(--foreground) 6%, transparent)",
  "--xy-minimap-background-color": "var(--card)",
  "--xy-minimap-mask-background-color": "color-mix(in srgb, var(--muted) 72%, transparent)",
  "--xy-minimap-mask-stroke-color": "var(--border)",
  "--xy-minimap-mask-stroke-width": "1",
  "--xy-minimap-node-background-color": "var(--secondary)",
  "--xy-minimap-node-stroke-color": "var(--border)",
  "--xy-minimap-node-stroke-width": "1",
  "--xy-selection-background-color": "color-mix(in srgb, var(--accent) 10%, transparent)",
  "--xy-selection-border": "1px dashed var(--primary)",
} as CSSProperties;

/* ── custom nodes ──────────────────────────────────────────── */
// centered, invisible handles so radial edges anchor node-center to node-center
function Pins() {
  const s = { left: "50%", top: "50%", opacity: 0, pointerEvents: "none" as const };
  return (
    <>
      <Handle type="target" position={Position.Top} style={s} isConnectable={false} />
      <Handle type="source" position={Position.Top} style={s} isConnectable={false} />
    </>
  );
}

function CoreNode({ selected }: NodeProps) {
  return (
    <div
      className={cn(
        "grid h-12 w-12 place-items-center rounded-2xl border bg-foreground text-background shadow-sm transition-[border-color,box-shadow] duration-200 hover:border-primary",
        selected ? "border-primary" : "border-foreground",
      )}
      aria-label="Centaury core"
    >
      <svg viewBox="164.156 191.1271 55.778 59.4725" className="h-6 w-6 fill-current" aria-hidden>
        <path d="M 196.355 212.88 L 188.131 221.28 L 196.256 229.48 L 204.479 221.28 C 204.28 220.78 196.355 212.88 196.355 212.88 Z M 182.385 193.48 C 181.791 200.88 175.648 207.78 166.831 208.18 C 164.75 212.38 163.165 222.88 166.633 233.28 L 166.93 233.68 L 167.128 234.08 C 175.549 234.08 181.89 240.88 182.484 248.18 C 185.654 249.48 190.013 250.28 193.283 250.58 C 202.894 250.88 212.404 247.78 219.736 239.28 L 219.835 239.28 L 212.009 231.28 C 203.191 241.38 193.581 240.88 186.447 238.08 C 184.367 231.88 177.134 223.58 164.156 221.28 C 173.37 219.48 181.592 214.38 185.654 204.88 C 192.292 201.68 203.092 201.38 211.711 211.18 L 219.934 203.48 L 219.835 203.38 L 219.835 203.28 L 219.736 203.28 L 219.24 202.58 C 215.872 198.38 210.026 194.78 204.875 192.78 C 199.723 190.98 192.095 189.98 182.385 193.38 L 182.385 193.48 Z" />
      </svg>
      <Pins />
    </div>
  );
}

function ProjectNode({ data, selected }: NodeProps) {
  const d = data as { name: string; count: number };
  return (
    <div
      className={cn(
        "flex w-[154px] items-center gap-2.5 rounded-2xl border bg-card p-3 transition-[border-color,box-shadow] duration-200 hover:border-primary hover:shadow-sm",
        selected ? "border-primary shadow-sm" : "border-border",
      )}
    >
      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-xl bg-muted text-primary">
        <FolderGit2 className="h-4 w-4" />
      </div>
      <div className="min-w-0 leading-tight">
        <div className="truncate font-heading text-sm font-semibold tracking-tight text-foreground">{d.name}</div>
        <div className="font-mono text-[11px] tabular-nums text-muted-foreground">
          {d.count} {d.count === 1 ? "instance" : "instances"}
        </div>
      </div>
      <Pins />
    </div>
  );
}

function HumanNode({ data, selected }: NodeProps) {
  const d = data as { name: string };
  return (
    <div className="group flex flex-col items-center gap-1.5">
      <div
        className={cn(
          "rounded-full border-2 bg-card p-0.5 transition-[border-color,box-shadow] duration-200 group-hover:border-primary group-hover:shadow-sm",
          selected ? "border-primary shadow-sm" : "border-foreground/25",
        )}
      >
        <AgentAvatar seed={d.name} size={42} className="rounded-full" />
      </div>
      <span className="rounded-full border border-border bg-card px-2.5 py-0.5 font-heading text-[11px] font-semibold leading-tight tracking-tight text-foreground shadow-2xs">
        {d.name}
      </span>
      <Pins />
    </div>
  );
}

function AgentNode({ data, selected }: NodeProps) {
  const d = data as { name: string; tool: string | null; tag: string | null; status: string; ctx: string };
  return (
    <div className="group flex flex-col items-center gap-0.5" title={d.ctx || d.name}>
      <div
        className={cn(
          "rounded-2xl border bg-card p-1 transition-[border-color,box-shadow] duration-200 group-hover:border-primary group-hover:shadow-sm",
          selected ? "border-primary shadow-sm" : "border-border",
        )}
      >
        <PixelSprite tool={d.tool ?? "claude"} size={28} />
      </div>
      <span className="flex items-center gap-1 rounded-full bg-card/90 px-1.5 py-0.5 font-mono text-[10px] leading-none text-muted-foreground">
        <span className={cn(d.status === "active" && "text-foreground")}>
          {d.tag ? `${d.tag}-` : ""}
          {d.name}
        </span>
        <span
          className={cn(
            "h-1.5 w-1.5 shrink-0 rounded-full",
            d.status === "active" ? "bg-status-ok" : "bg-muted-foreground/45",
          )}
        />
      </span>
      <Pins />
    </div>
  );
}

const nodeTypes = { core: CoreNode, project: ProjectNode, human: HumanNode, agent: AgentNode };

function FitOnChange({ viewKey }: { viewKey: string }) {
  const { fitView } = useReactFlow();
  const nodesInitialized = useNodesInitialized();
  useEffect(() => {
    if (nodesInitialized) {
      fitView({ padding: 0.28, duration: 220 });
    }
  }, [viewKey, nodesInitialized, fitView]);
  return null;
}

/* ── graph builders (radial, in px) ────────────────────────── */
const CX = 440, CY = 340;

function orgGraph(projects: { name: string; count: number }[]): { nodes: Node[]; edges: Edge[] } {
  const R = 240;
  const nodes: Node[] = [{ id: "core", type: "core", position: { x: CX - 24, y: CY - 24 }, data: {}, draggable: false }];
  const edges: Edge[] = [];
  const N = Math.max(projects.length, 1);
  projects.forEach((p, i) => {
    const a = (-90 + (i * 360) / N) * (Math.PI / 180);
    nodes.push({
      id: `p:${p.name}`,
      type: "project",
      position: { x: CX + R * Math.cos(a) - 75, y: CY + R * Math.sin(a) - 30 },
      data: { name: p.name, count: p.count },
    });
    edges.push({ id: `e:${p.name}`, source: "core", target: `p:${p.name}`, type: "straight", style: EDGE_STYLE });
  });
  return { nodes, edges };
}

function projGraph(roster: InstanceSummary[]): { nodes: Node[]; edges: Edge[] } {
  const R1 = 220, R2 = 100;
  const humans = roster.filter((i) => i.kind === "human");
  const humanNames = new Set(humans.map((h) => h.name));
  const agentsByOwner: Record<string, InstanceSummary[]> = {};
  const loose: InstanceSummary[] = []; // agents whose owner is unknown (org directory) hang off the core
  for (const a of roster) {
    if (a.kind !== "agent") continue;
    if (a.owner && humanNames.has(a.owner)) (agentsByOwner[a.owner] ??= []).push(a);
    else loose.push(a);
  }

  const nodes: Node[] = [{ id: "core", type: "core", position: { x: CX - 24, y: CY - 24 }, data: {}, draggable: false }];
  const edges: Edge[] = [];
  const agentNode = (a: InstanceSummary, x: number, y: number): Node => ({
    id: a.name,
    type: "agent",
    position: { x: x - 22, y: y - 22 },
    data: { name: a.name, tool: a.tool, tag: a.tag, status: a.status, ctx: a.status_context ?? "" },
  });
  const H = Math.max(humans.length, 1);
  humans.forEach((h, i) => {
    const ang = (-90 + (i * 360) / H) * (Math.PI / 180);
    const hx = CX + R1 * Math.cos(ang), hy = CY + R1 * Math.sin(ang);
    nodes.push({ id: h.name, type: "human", position: { x: hx - 22, y: hy - 30 }, data: { name: h.name } });
    edges.push({ id: `e:${h.name}`, source: "core", target: h.name, type: "straight", style: EDGE_STYLE });
    const mine = agentsByOwner[h.name] ?? [];
    const outward = Math.atan2(hy - CY, hx - CX);
    mine.forEach((a, j) => {
      const aang = outward + (j - (mine.length - 1) / 2) * 0.6;
      nodes.push(agentNode(a, hx + R2 * Math.cos(aang), hy + R2 * Math.sin(aang)));
      edges.push({ id: `e:${a.name}`, source: h.name, target: a.name, type: "straight", style: AGENT_EDGE_STYLE });
    });
  });
  const RL = humans.length ? 110 : 190;
  loose.forEach((a, i) => {
    const ang = (-90 + (i * 360) / Math.max(loose.length, 1)) * (Math.PI / 180);
    nodes.push(agentNode(a, CX + RL * Math.cos(ang), CY + RL * Math.sin(ang)));
    edges.push({ id: `e:${a.name}`, source: "core", target: a.name, type: "straight", style: AGENT_EDGE_STYLE });
  });
  return { nodes, edges };
}

/* ── legend: what each line and colour means ───────────────── */
function Legend({ level, directory }: { level: "org" | "project"; directory?: boolean }) {
  const line = (dash: string) => (
    <svg width="22" height="6" aria-hidden className="shrink-0">
      <line x1="0" y1="3" x2="22" y2="3" stroke={EDGE_STROKE} strokeWidth="1.5" strokeDasharray={dash} />
    </svg>
  );
  return (
    <ul aria-label="Legend" className="flex flex-wrap items-center gap-x-5 gap-y-1 text-xs text-muted-foreground">
      {level === "org" ? (
        <li className="flex items-center gap-2">{line("4 4")} project belongs to the organization (Centaury core)</li>
      ) : directory ? (
        <>
          <li className="flex items-center gap-2">{line("3 3")} agent in this project, linked to the Centaury core</li>
          <li><StatusDot tone="ok">active</StatusDot></li>
          <li><StatusDot tone="idle">inactive</StatusDot></li>
        </>
      ) : (
        <>
          <li className="flex items-center gap-2">{line("4 4")} person in this project</li>
          <li className="flex items-center gap-2">{line("3 3")} agent, linked to the person who owns it</li>
          <li><StatusDot tone="ok">active</StatusDot></li>
          <li><StatusDot tone="idle">inactive</StatusDot></li>
        </>
      )}
    </ul>
  );
}

/* ── list view: the same rows the graph draws ──────────────── */
function OrgList({ projects, onPick }: { projects: ProjectSummary[]; onPick: (name: string) => void }) {
  return (
    <DataTable
      className="p-4"
      minWidth={420}
      rows={projects}
      rowKey={(p) => p.name}
      columns={[
        { header: "Project", cell: (p) => (
          <button type="button" onClick={() => onPick(p.name)} className="font-medium underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-primary">
            {p.name}
          </button>
        ) },
        { header: "Instances", mono: true, cell: (p) => p.instance_count },
        { header: "Linked to", cell: () => <span className="text-muted-foreground">Centaury core</span> },
      ]}
    />
  );
}

function ProjectList({ roster, linkedTo }: { roster: InstanceSummary[]; linkedTo: (i: InstanceSummary) => string }) {
  return (
    <DataTable
      className="p-4"
      minWidth={560}
      rows={roster}
      rowKey={(i) => `${i.kind}:${i.name}`}
      columns={[
        { header: "Name", cell: (i) => <span className="font-medium">{i.tag ? `${i.tag}-` : ""}{i.name}</span> },
        { header: "Kind", cell: (i) => <span className="capitalize">{i.kind}</span> },
        { header: "Linked to", cell: (i) => <span className="text-muted-foreground">{linkedTo(i)}</span> },
        { header: "Tool", mono: true, cell: (i) => i.tool ?? "—" },
        { header: "Status", cell: (i) => <StatusDot tone={i.status === "active" ? "ok" : "idle"}><span className="capitalize">{i.status}</span></StatusDot> },
      ]}
    />
  );
}

/* ── page ──────────────────────────────────────────────────── */
type Fail = { kind: ErrorKind; message: string };
type View = "Graph" | "List";

export function Network({ onLogin, onOpenProject }: { onLogin?: () => void; onOpenProject?: (name: string) => void }) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [sel, setSel] = useState<string | "org">("org");
  const [view, setView] = useState<View>("Graph");
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  const [directory, setDirectory] = useState(false); // roster came from /v1/org/agents (no owners/tools)
  const [projState, setProjState] = useState<"loading" | "ready" | "error">(store.demo ? "ready" : "loading");
  const [projErr, setProjErr] = useState<Fail | null>(null);
  const [rosterState, setRosterState] = useState<"loading" | "ready" | "error">("ready");
  const [rosterErr, setRosterErr] = useState<Fail | null>(null);

  // org project list (real: name + instance_count)
  const loadProjects = useCallback(() => {
    if (store.demo) {
      setProjects(mockOrg.projects.map((p) => ({ name: p.name, instance_count: p.agents })));
      return;
    }
    if (!store.session) return;
    setProjErr(null);
    setProjState("loading");
    api
      .userProjects(store.session)
      .then((p) => {
        setProjects(p);
        setProjState("ready");
      })
      .catch((e) => {
        setProjErr(errorInfo(e));
        setProjState("error");
      });
  }, []);
  useEffect(loadProjects, [loadProjects]);

  // drill: the project's own token when we have one, else the org-wide directory.
  const loadRoster = useCallback(() => {
    if (sel === "org") return;
    setRosterErr(null);
    setDirectory(false);
    if (store.demo) {
      setRoster(mockRoster);
      setRosterState("ready");
      return;
    }
    setRoster([]);
    setRosterState("loading");
    const token = store.tokenFor(sel);
    const fromDirectory = () =>
      api.orgAgents(store.session ?? "").then((all) => {
        setDirectory(true);
        return all
          .filter((a) => a.project === sel)
          .map<InstanceSummary>((a) => ({
            name: a.name, tag: null, status: a.status, kind: "agent", owner: null, tool: null,
            directory: null, status_context: a.role ?? "", last_seen_msg_id: 0,
          }));
      });
    (token ? api.instances(token).catch((e) => (errorInfo(e).kind === "auth" ? fromDirectory() : Promise.reject(e))) : fromDirectory())
      .then((r) => {
        setRoster(r);
        setRosterState("ready");
      })
      .catch((e) => {
        setRosterErr(errorInfo(e));
        setRosterState("error");
      });
  }, [sel]);
  useEffect(loadRoster, [loadRoster]);

  const graph = useMemo(
    () =>
      sel === "org"
        ? orgGraph(projects.map((p) => ({ name: p.name, count: p.instance_count })))
        : projGraph(roster),
    [sel, projects, roster],
  );

  const [nodes, setNodes, onNodesChange] = useNodesState(graph.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(graph.edges);
  useEffect(() => {
    setNodes(graph.nodes);
    setEdges(graph.edges);
  }, [graph, setNodes, setEdges]);

  // click a project node → drill in
  const onNodeClick = useCallback((_: unknown, node: Node) => {
    if (node.type === "project") setSel((node.data as { name: string }).name);
  }, []);

  const levelTabs = useMemo(
    () => [
      { label: "Organization", count: projects.length },
      ...projects.map((p) => ({ label: p.name, count: p.instance_count })),
    ],
    [projects],
  );

  const failed = sel === "org" ? projState === "error" : rosterState === "error";
  const loading = sel === "org" ? projState === "loading" : rosterState === "loading";
  const err = sel === "org" ? projErr : rosterErr;
  const empty = !failed && !loading && (sel === "org" ? projects.length === 0 : roster.length === 0);
  const humanNames = new Set(roster.filter((r) => r.kind === "human").map((r) => r.name));
  const linkedTo = (i: InstanceSummary) => (i.kind === "agent" && i.owner && humanNames.has(i.owner) ? i.owner : "Centaury core");

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col gap-3 p-2">
      <header className="flex flex-col gap-3">
        <div className="flex items-center gap-2.5">
          {sel !== "org" && (
            <IconButton label="Back to organization" onClick={() => setSel("org")}>
              <ArrowLeft size={14} />
            </IconButton>
          )}
          <div className="min-w-0 flex-1">
            <PanelHeader title={sel === "org" ? "Network" : sel}>
              {store.demo && (
                <span className="inline-flex items-center rounded-full border border-border bg-muted px-2.5 py-1 text-[11px] font-medium text-muted-foreground">
                  <StatusDot tone="pending">demo</StatusDot>
                </span>
              )}
              {sel !== "org" && onOpenProject && (
                <Button size="sm" variant="outline" onClick={() => onOpenProject(sel)}>
                  <ArrowUpRight /> Open project
                </Button>
              )}
            </PanelHeader>
            <p className="mt-0.5 text-xs text-muted-foreground">
              {sel === "org"
                ? "Projects across the organization — pick one to see who is in it"
                : "Who is in this project and who owns which agent"}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center justify-between gap-3">
          <TabPills
            tabs={levelTabs}
            value={sel === "org" ? "Organization" : sel}
            onChange={(tab) => setSel(tab === "Organization" ? "org" : tab)}
          />
          <TabPills tabs={[{ label: "Graph" as const }, { label: "List" as const }]} value={view} onChange={setView} />
        </div>
        <Legend level={sel === "org" ? "org" : "project"} directory={directory} />
        {sel !== "org" && directory && (
          <p className="text-xs text-muted-foreground">
            Agents only (organization directory): people, owners and tools appear once you open the project.
          </p>
        )}
      </header>

      <div
        style={FLOW_VARS}
        className="relative min-h-0 flex-1 overflow-hidden rounded-2xl border border-border bg-muted"
      >
        {failed || loading ? (
          <QueryState
            className="absolute inset-0 z-20 bg-muted"
            state={failed ? "error" : "loading"}
            message={failed ? err?.message : "Loading…"}
            kind={err?.kind}
            onRetry={sel === "org" ? loadProjects : loadRoster}
            onLogin={onLogin}
          />
        ) : empty ? (
          <QueryState
            className="absolute inset-0 z-20 bg-muted"
            state="empty"
            message={sel === "org" ? "No projects yet. Join one from Home." : "Nobody has joined this project yet."}
          />
        ) : view === "List" ? (
          <div className="absolute inset-0 z-10 overflow-auto bg-card">
            {sel === "org" ? <OrgList projects={projects} onPick={setSel} /> : <ProjectList roster={roster} linkedTo={linkedTo} />}
          </div>
        ) : null}
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          fitView
          fitViewOptions={{ padding: 0.28 }}
          minZoom={0.3}
          proOptions={{ hideAttribution: true }}
        >
          <Background
            variant={BackgroundVariant.Dots}
            color="color-mix(in srgb, var(--foreground) 15%, transparent)"
            gap={22}
            size={1.25}
          />
          <Controls
            showInteractive={false}
            fitViewOptions={{ padding: 0.28 }}
            className="overflow-hidden rounded-xl border border-border [&>button:last-child]:border-b-0"
          />
          <MiniMap
            pannable
            zoomable
            style={{ width: 120, height: 84 }}
            nodeColor={(n) => (n.type === "core" ? "var(--foreground)" : "var(--secondary)")}
            nodeStrokeColor="var(--border)"
            maskColor="color-mix(in srgb, var(--muted) 75%, transparent)"
            className="overflow-hidden rounded-xl border border-border"
          />
          <FitOnChange viewKey={sel} />
        </ReactFlow>
      </div>
    </div>
  );
}
