// Network — the collaboration graph, its own page (moved out of the workspace).
// Two levels: (1) Organization = projects orbiting the Kore core (real data:
// name + instance_count); click a project to drill in. (2) Project = its roster
// as a graph — humans (owners) ringed, their agents orbiting them, all wired to
// the core. Built on React Flow (@xyflow/react): pan / zoom / drag / minimap /
// select for free. Node positions are computed here (radial); React Flow layers
// the interaction on top — no auto-layout dep.
//
// Data limits (thin client): /v1/instances only exposes the JOINED project's
// roster, so live drill works for store.project; other projects need an
// org-wide roster endpoint (server follow-up). Demo shows the full mock bubble.
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  ReactFlow,
  Background,
  Controls,
  MiniMap,
  Handle,
  Position,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { ArrowLeft, Cpu, FolderGit2 } from "lucide-react";
import { api, connectWs, store, type Delivery, type InstanceSummary, type ProjectSummary } from "@/lib/api";
import { mockOrg, mockRoster } from "@/lib/mock";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite } from "@/components/kore/sprites";

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

function KoreNode() {
  return (
    <div className="grid h-12 w-12 place-items-center rounded-xl bg-primary font-mono text-lg font-bold text-primary-foreground shadow-lg">
      K<Pins />
    </div>
  );
}

function ProjectNode({ data }: NodeProps) {
  const d = data as { name: string; count: number };
  return (
    <div className="flex w-[150px] items-center gap-3 rounded-2xl border border-border bg-card p-3 shadow-sm transition-colors hover:border-foreground/25">
      <div className="grid h-9 w-9 shrink-0 place-items-center rounded-lg bg-muted text-muted-foreground">
        <FolderGit2 className="h-4 w-4" />
      </div>
      <div className="min-w-0 leading-tight">
        <div className="truncate text-sm font-semibold tracking-tight">{d.name}</div>
        <div className="text-[11px] text-muted-foreground">{d.count} instances</div>
      </div>
      <Pins />
    </div>
  );
}

function HumanNode({ data, selected }: NodeProps) {
  const d = data as { name: string };
  return (
    <div className="flex flex-col items-center gap-1">
      <div className={selected ? "rounded-full ring-2 ring-primary" : "rounded-full ring-2 ring-background"}>
        <AgentAvatar seed={d.name} size={44} className="rounded-full" />
      </div>
      <span className="font-mono text-[11px] font-semibold">{d.name}</span>
      <Pins />
    </div>
  );
}

function AgentNode({ data, selected }: NodeProps) {
  const d = data as { name: string; tool: string | null; tag: string | null; status: string; ctx: string };
  return (
    <div className="flex flex-col items-center gap-0.5" title={d.ctx || d.name}>
      <div
        className="rounded-md border-2 bg-card p-1"
        style={{ borderColor: selected ? "var(--primary)" : "var(--border)" }}
      >
        <PixelSprite tool={d.tool ?? "claude"} size={34} />
      </div>
      <span className="flex items-center gap-1 font-mono text-[10px] leading-none text-muted-foreground">
        {d.tag ? `${d.tag}-` : ""}
        {d.name}
        <span
          className="h-1.5 w-1.5 rounded-full"
          style={{ background: d.status === "active" ? "#22c55e" : "var(--muted-foreground)" }}
        />
      </span>
    </div>
  );
}

const nodeTypes = { kore: KoreNode, project: ProjectNode, human: HumanNode, agent: AgentNode };

/* ── graph builders (radial, in px) ────────────────────────── */
const CX = 440, CY = 340;

function orgGraph(projects: { name: string; count: number }[]): { nodes: Node[]; edges: Edge[] } {
  const R = 240;
  const nodes: Node[] = [{ id: "kore", type: "kore", position: { x: CX - 24, y: CY - 24 }, data: {}, draggable: false }];
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
    edges.push({ id: `e:${p.name}`, source: "kore", target: `p:${p.name}`, type: "straight", style: { stroke: "var(--border)" } });
  });
  return { nodes, edges };
}

function projGraph(roster: InstanceSummary[]): { nodes: Node[]; edges: Edge[] } {
  const R1 = 220, R2 = 100;
  const humans = roster.filter((i) => i.kind === "human");
  const agentsByOwner: Record<string, InstanceSummary[]> = {};
  for (const a of roster) if (a.kind === "agent") (agentsByOwner[a.owner ?? "?"] ??= []).push(a);

  const nodes: Node[] = [{ id: "kore", type: "kore", position: { x: CX - 24, y: CY - 24 }, data: {}, draggable: false }];
  const edges: Edge[] = [];
  const H = Math.max(humans.length, 1);
  humans.forEach((h, i) => {
    const ang = (-90 + (i * 360) / H) * (Math.PI / 180);
    const hx = CX + R1 * Math.cos(ang), hy = CY + R1 * Math.sin(ang);
    nodes.push({ id: h.name, type: "human", position: { x: hx - 22, y: hy - 30 }, data: { name: h.name } });
    edges.push({ id: `e:${h.name}`, source: "kore", target: h.name, type: "straight", style: { stroke: "var(--border)" } });
    const mine = agentsByOwner[h.name] ?? [];
    const outward = Math.atan2(hy - CY, hx - CX);
    mine.forEach((a, j) => {
      const spread = (j - (mine.length - 1) / 2) * 0.6;
      const aang = outward + spread;
      nodes.push({
        id: a.name,
        type: "agent",
        position: { x: hx + R2 * Math.cos(aang) - 22, y: hy + R2 * Math.sin(aang) - 22 },
        data: { name: a.name, tool: a.tool, tag: a.tag, status: a.status, ctx: a.status_context ?? "" },
      });
      edges.push({ id: `e:${a.name}`, source: h.name, target: a.name, type: "straight", style: { stroke: "var(--border)", strokeDasharray: "3 3" } });
    });
  });
  return { nodes, edges };
}

/* ── page ──────────────────────────────────────────────────── */
export function Network() {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [sel, setSel] = useState<string | "org">("org");
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  const [rosterErr, setRosterErr] = useState<string | null>(null);

  // org project list (real: name + instance_count)
  useEffect(() => {
    if (store.demo) {
      setProjects(mockOrg.projects.map((p) => ({ name: p.name, instance_count: p.agents })));
    } else if (store.session) {
      api.userProjects(store.session).then(setProjects).catch(() => {});
    }
  }, []);

  // drill: fetch the project roster. Live only works for the joined project.
  useEffect(() => {
    if (sel === "org") return;
    setRosterErr(null);
    if (store.demo) {
      setRoster(mockRoster);
      return;
    }
    if (sel !== store.project) {
      setRoster([]);
      setRosterErr(`You only see the roster of the project you joined (@${store.project}). Org-wide agents need a server endpoint (DU-S).`);
      return;
    }
    api.instances().then(setRoster).catch((e) => setRosterErr(String(e)));
  }, [sel]);

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

  // live traffic pulse (project view, joined project): light up edges on a
  // delivery — broadcast → every agent edge from sender; mention → that edge.
  useEffect(() => {
    if (sel === "org" || (!store.demo && sel !== store.project)) return;
    let off: (() => void) | undefined;
    let last: number | null = null;
    const pulse = (ids: string[]) => {
      setEdges((es) => es.map((e) => (ids.includes(e.id) ? { ...e, animated: true, style: { ...e.style, stroke: "var(--primary)" } } : e)));
      window.setTimeout(
        () => setEdges((es) => es.map((e) => (ids.includes(e.id) ? { ...e, animated: false, style: { ...e.style, stroke: "var(--border)" } } : e))),
        1400,
      );
    };
    const onDelivery = (d: Delivery) => {
      if (last === null) { last = d.id; return; }
      if (d.id <= last) return;
      last = d.id;
      const targets = d.scope === "broadcast" ? roster.filter((r) => r.kind === "agent").map((r) => r.name) : d.mentions;
      pulse(targets.map((t) => `e:${t}`));
    };
    connectWs(onDelivery).then((fn) => (off = fn)).catch(() => {});
    return () => off?.();
  }, [sel, roster, setEdges]);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b px-6 py-4">
        {sel !== "org" && (
          <button onClick={() => setSel("org")} className="flex h-8 w-8 items-center justify-center rounded-full hover:bg-muted">
            <ArrowLeft className="h-4 w-4" />
          </button>
        )}
        <div className="min-w-0 flex-1">
          <h1 className="k-title text-lg">{sel === "org" ? "Network" : sel}</h1>
          <p className="text-xs text-muted-foreground">
            {sel === "org" ? "Projects across the organization — click one to drill in" : "Roster graph — humans own the agents that orbit them"}
          </p>
        </div>
        {/* selector */}
        <select
          value={sel}
          onChange={(e) => setSel(e.target.value)}
          className="rounded-full border border-border bg-card px-3 py-1.5 text-sm outline-none"
        >
          <option value="org">Organization</option>
          {projects.map((p) => (
            <option key={p.name} value={p.name}>{p.name}</option>
          ))}
        </select>
      </header>

      <div className="relative min-h-0 flex-1">
        {rosterErr && (
          <div className="absolute left-1/2 top-1/2 z-10 -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border bg-card px-5 py-4 text-center text-sm text-muted-foreground shadow-sm">
            <Cpu className="mx-auto mb-2 h-5 w-5 opacity-60" />
            {rosterErr}
          </div>
        )}
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeClick={onNodeClick}
          fitView
          fitViewOptions={{ padding: 0.2 }}
          minZoom={0.3}
          proOptions={{ hideAttribution: true }}
        >
          <Background color="var(--border)" gap={22} />
          <Controls showInteractive={false} />
          <MiniMap pannable zoomable className="!bg-muted" maskColor="rgba(0,0,0,0.06)" />
        </ReactFlow>
      </div>
    </div>
  );
}
