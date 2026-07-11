// CollabCanvas — the project's collaboration made visual. Seeded by the roster:
// humans (owners) sit on the outer ring, each of their agents orbits them (agents
// listen only to their owner), everything wired to the kore core (the bus). A
// message = an orb travelling sender→core→recipient; a broadcast fans to all
// nodes. Agents are pixelart (sprites.tsx); humans are circular avatars.
// DU-D7: orbs come from REAL traffic (`traffic` = live feed from the parent's
// peek WS; fan-out per §2 rule 2 = scope+mentions, never delivered_to).
// `ambient` keeps the old faked traffic for demo mode only.
import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { PixelSprite, TOOL_MAP } from "./sprites";
import type { Delivery, InstanceSummary } from "@/lib/api";

type Node = { name: string; kind: string; tool: string | null; owner: string | null; x: number; y: number };
type Orb = { id: number; from: Node; to: Node; color: string };

const CENTER = { x: 50, y: 50 };
const R1 = 30; // owner ring
const R2 = 13; // agents around their owner

export function CollabCanvas({ roster, traffic = [], ambient = false, broadcastNonce = 0 }: { roster: InstanceSummary[]; traffic?: Delivery[]; ambient?: boolean; broadcastNonce?: number }) {
  const nodes = useMemo<Node[]>(() => build(roster), [roster]);
  const byName = useMemo(() => Object.fromEntries(nodes.map((n) => [n.name, n])), [nodes]);
  const humanOf = (n: Node) => (n.kind === "human" ? n : n.owner ? byName[n.owner] : undefined);

  const [orbs, setOrbs] = useState<Orb[]>([]);
  const [hot, setHot] = useState<Set<string>>(new Set());
  const orbId = useRef(0);
  const hotTimers = useRef<Record<string, number>>({});

  function markHot(name: string) {
    setHot((h) => new Set(h).add(name));
    window.clearTimeout(hotTimers.current[name]);
    hotTimers.current[name] = window.setTimeout(() => setHot((h) => {
      const next = new Set(h);
      next.delete(name);
      return next;
    }), 650);
  }
  function color(n: Node) {
    return n.tool ? (TOOL_MAP[n.tool]?.body ?? "#888") : "var(--primary)";
  }
  function emit(from?: Node, to?: Node) {
    if (!from || !to || from.name === to.name) return;
    setOrbs((o) => [...o, { id: orbId.current++, from, to, color: color(from) }]);
    markHot(from.name);
    markHot(to.name);
  }

  // real traffic (DU-D7): animate only deliveries that arrive AFTER mount —
  // replaying the whole history as one orb burst is noise, not information.
  const lastSeen = useRef<number | null>(null);
  useEffect(() => {
    if (lastSeen.current === null) {
      lastSeen.current = traffic.reduce((m, d) => Math.max(m, d.id), 0);
      return;
    }
    for (const d of traffic) {
      if (d.id <= lastSeen.current) continue;
      lastSeen.current = d.id;
      const from = byName[d.from];
      if (d.scope === "broadcast") {
        nodes.filter((n) => n.name !== d.from).forEach((to) => emit(from, to));
      } else {
        d.mentions.forEach((m) => emit(from, byName[m]));
      }
    }
  }, [traffic, byName, nodes]);

  // ambient fake traffic — demo mode only (live projects show the real thing)
  useEffect(() => {
    if (!ambient || nodes.length < 2) return;
    const pick = () => nodes[Math.floor(Math.random() * nodes.length)];
    const iv = window.setInterval(() => {
      const from = pick();
      if (Math.random() < 0.18) {
        // broadcast: fan to all agents
        nodes.filter((n) => n.kind === "agent" && n.name !== from.name).forEach((to) => emit(from, to));
      } else {
        emit(from, pick());
      }
    }, 1500);
    return () => window.clearInterval(iv);
  }, [ambient, nodes]);

  // broadcast composer → fan from "you" (or first human) to every agent
  useEffect(() => {
    if (!broadcastNonce) return;
    const me = nodes.find((n) => n.name === "you") ?? nodes.find((n) => n.kind === "human");
    nodes.filter((n) => n.kind === "agent").forEach((to) => emit(me, to));
  }, [broadcastNonce]);

  return (
    <div className="relative mx-auto aspect-square w-full max-w-[620px] flex-1 self-center p-4">
      <div className="relative h-full w-full">
        {/* wires */}
        <svg className="absolute inset-0 h-full w-full text-muted-foreground/25" viewBox="0 0 100 100" preserveAspectRatio="none">
          {nodes.map((n) => {
            const h = humanOf(n);
            const t = n.kind === "human" ? CENTER : h ?? CENTER;
            return <line key={n.name} x1={n.x} y1={n.y} x2={t.x} y2={t.y} stroke="currentColor" strokeWidth={n.kind === "human" ? 0.6 : 0.4} vectorEffect="non-scaling-stroke" strokeDasharray={n.kind === "human" ? undefined : "1 1.5"} />;
          })}
        </svg>

        {/* kore core */}
        <div className="absolute grid h-11 w-11 -translate-x-1/2 -translate-y-1/2 place-items-center rounded-md bg-primary font-mono text-lg font-bold text-primary-foreground shadow-lg" style={{ left: `${CENTER.x}%`, top: `${CENTER.y}%` }}>
          K
        </div>

        {/* nodes */}
        {nodes.map((n) => {
          const isHot = hot.has(n.name);
          const isHuman = n.kind === "human";
          const size = isHuman ? 40 : 34;
          return (
            <div key={n.name} className="absolute flex -translate-x-1/2 -translate-y-1/2 flex-col items-center gap-0.5" style={{ left: `${n.x}%`, top: `${n.y}%` }} title={n.name}>
              <motion.div
                animate={{ scale: isHot ? 1.18 : 1 }}
                transition={{ type: "spring", stiffness: 400, damping: 18 }}
                className={isHuman ? "rounded-full ring-2 ring-background" : "rounded-md border-2 bg-card p-1"}
                style={isHuman ? { boxShadow: isHot ? "0 0 12px var(--primary)" : undefined } : { borderColor: isHot ? color(n) : "var(--border)", boxShadow: isHot ? `0 0 12px ${color(n)}80` : undefined }}
              >
                {isHuman ? <AgentAvatar seed={n.name} size={size} className="rounded-full" /> : <PixelSprite tool={n.tool ?? "claude"} size={size} />}
              </motion.div>
              <span className={`font-mono text-[10px] leading-none ${isHuman ? "font-semibold" : "text-muted-foreground"}`}>{n.name}</span>
            </div>
          );
        })}

        {/* orbs */}
        <AnimatePresence>
          {orbs.map((orb) => (
            <motion.div
              key={orb.id}
              className="pointer-events-none absolute h-2.5 w-2.5 rounded-full"
              style={{ translateX: "-50%", translateY: "-50%", background: orb.color, boxShadow: `0 0 8px ${orb.color}` }}
              initial={{ left: `${orb.from.x}%`, top: `${orb.from.y}%`, opacity: 0 }}
              animate={{ left: [`${orb.from.x}%`, `${CENTER.x}%`, `${orb.to.x}%`], top: [`${orb.from.y}%`, `${CENTER.y}%`, `${orb.to.y}%`], opacity: [0, 1, 1, 0] }}
              transition={{ duration: 1.4, times: [0, 0.5, 1], ease: "easeInOut" }}
              onAnimationComplete={() => setOrbs((o) => o.filter((x) => x.id !== orb.id))}
            />
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

function build(roster: InstanceSummary[]): Node[] {
  const humans = roster.filter((i) => i.kind === "human");
  const agentsByOwner: Record<string, InstanceSummary[]> = {};
  for (const a of roster) if (a.kind === "agent") (agentsByOwner[a.owner ?? "?"] ??= []).push(a);

  const nodes: Node[] = [];
  const H = Math.max(humans.length, 1);
  humans.forEach((h, i) => {
    const ang = (-90 + (i * 360) / H) * (Math.PI / 180);
    const hx = CENTER.x + R1 * Math.cos(ang);
    const hy = CENTER.y + R1 * Math.sin(ang);
    nodes.push({ name: h.name, kind: "human", tool: null, owner: null, x: hx, y: hy });
    const mine = agentsByOwner[h.name] ?? [];
    const outward = Math.atan2(hy - CENTER.y, hx - CENTER.x);
    mine.forEach((a, j) => {
      const spread = (j - (mine.length - 1) / 2) * 0.6;
      const aang = outward + spread;
      nodes.push({ name: a.name, kind: "agent", tool: a.tool, owner: h.name, x: hx + R2 * Math.cos(aang), y: hy + R2 * Math.sin(aang) });
    });
  });
  return nodes;
}
