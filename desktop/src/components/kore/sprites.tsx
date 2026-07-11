// Hand-authored pixelart sprites — one robot per tool dialect, brand-tinted.
// Pure inline SVG (crispEdges), zero deps, zero assets, theme-agnostic. Used by
// Wire.tsx (the fleet visualization). Colors are decorative brand-evocations,
// not official values. One shared 12x12 robot grid + a human grid; the tool's
// palette (body/dark/eye) is what makes each node read distinct.
// ponytail: single silhouette recolored per tool; give a tool its own grid here
// if color alone stops being enough to tell them apart.

type Grid = string[];

// '.' empty · 'X' body · 'o' outline/shadow · 'e' eye
const ROBOT: Grid = [
  "...o....o...",
  "...o....o...",
  "..oooooooo..",
  "..oXXXXXXo..",
  "..oXeXXeXo..",
  "..oXXXXXXo..",
  "..oXooooXo..",
  "..oXXXXXXo..",
  "..oooooooo..",
  "....XXXX....",
  ".oXXXXXXXXo.",
  ".oXXXXXXXXo.",
];

const HUMAN: Grid = [
  "............",
  "....oooo....",
  "...oXXXXo...",
  "..oXXXXXXo..",
  "..oXeXXeXo..",
  "..oXXXXXXo..",
  "...oXXXXo...",
  "....oXXo....",
  "..oXXXXXXo..",
  ".oXXXXXXXXo.",
  ".oXXXXXXXXo.",
  ".oXX....XXo.",
];

export type Tool = {
  id: string;
  label: string;
  body: string;
  dark: string;
  eye: string;
  grid?: Grid; // defaults to ROBOT
};

// The tool zoo — mirrors the 10 hook-dialect tools kore-client speaks to.
export const TOOLS: Tool[] = [
  { id: "claude", label: "Claude", body: "#D97757", dark: "#A94F35", eye: "#FFE7CC" },
  { id: "codex", label: "Codex", body: "#10A37F", dark: "#0B6B52", eye: "#B6F3DE" },
  { id: "gemini", label: "Gemini CLI", body: "#4285F4", dark: "#2B5AB0", eye: "#D2E3FF" },
  { id: "cursor", label: "Cursor", body: "#3B3F46", dark: "#17191D", eye: "#C7CBD1" },
  { id: "copilot", label: "Copilot CLI", body: "#6E40C9", dark: "#4A2A8C", eye: "#DCC9F7" },
  { id: "kimi", label: "Kimi CLI", body: "#0EA5E9", dark: "#0A6FA0", eye: "#C4ECFF" },
  { id: "opencode", label: "opencode", body: "#F59E0B", dark: "#B45309", eye: "#FDE9BE" },
  { id: "kilo", label: "Kilo Code", body: "#8B5CF6", dark: "#5E3AB0", eye: "#DED0FB" },
  { id: "cline", label: "Cline", body: "#22C55E", dark: "#158040", eye: "#BDF0CF" },
  { id: "antigravity", label: "Antigravity", body: "#6366F1", dark: "#3F41B0", eye: "#CFD1FB" },
];

export const human: Tool = { id: "human", label: "You", body: "#64748B", dark: "#3E4753", eye: "#E2E8F0", grid: HUMAN };

export const TOOL_MAP: Record<string, Tool> = Object.fromEntries(
  [...TOOLS, human].map((t) => [t.id, t]),
);

export function PixelSprite({ tool, size = 48 }: { tool: string; size?: number }) {
  const t = TOOL_MAP[tool] ?? TOOL_MAP.claude;
  const grid = t.grid ?? ROBOT;
  const fill = (c: string) => (c === "X" ? t.body : c === "o" ? t.dark : c === "e" ? t.eye : null);
  return (
    <svg width={size} height={size} viewBox="0 0 12 12" shapeRendering="crispEdges" aria-label={t.label}>
      {grid.flatMap((row, y) =>
        [...row].map((ch, x) => {
          const f = fill(ch);
          return f ? <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill={f} /> : null;
        }),
      )}
    </svg>
  );
}
