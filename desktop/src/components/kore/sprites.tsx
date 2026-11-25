// Hand-authored pixelart sprites — one robot per tool dialect, brand-tinted.
// Pure inline SVG (crispEdges), zero deps, zero assets, theme-agnostic. Used by
// Wire.tsx (the fleet visualization). The website (web/src/components/agents/identity.tsx)
// identifies agents by icon marks, not figurative robots. We follow a simplified mark
// approach: same robot silhouette but restricted to the slate/brand ramp per RULES.md §6.
// palette: each tool picks three values from the ramp so the workspace stays monochrome.
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

// Slate/brand ramp — the only allowed palette per RULES.md §3.
// Each tool picks a distinct lightness step so they stay legible at small sizes.
// Ramp: #2c2e3a · #41465e · #595e7a · #6c7296 · #777fa8 · #959fbf · #abb6ce · #c9d1e0 · #dfe6f5
export const TOOLS: Tool[] = [
  { id: "claude",      label: "Claude",       body: "#595e7a", dark: "#2c2e3a", eye: "#dfe6f5" },
  { id: "codex",       label: "Codex",        body: "#6c7296", dark: "#41465e", eye: "#dfe6f5" },
  { id: "gemini",      label: "Gemini CLI",   body: "#777fa8", dark: "#41465e", eye: "#dfe6f5" },
  { id: "cursor",      label: "Cursor",       body: "#41465e", dark: "#2c2e3a", eye: "#c9d1e0" },
  { id: "copilot",     label: "Copilot CLI",  body: "#959fbf", dark: "#595e7a", eye: "#dfe6f5" },
  { id: "kimi",        label: "Kimi CLI",     body: "#abb6ce", dark: "#6c7296", eye: "#dfe6f5" },
  { id: "opencode",    label: "opencode",     body: "#595e7a", dark: "#2c2e3a", eye: "#abb6ce" },
  { id: "kilo",        label: "Kilo Code",    body: "#6c7296", dark: "#2c2e3a", eye: "#c9d1e0" },
  { id: "cline",       label: "Cline",        body: "#777fa8", dark: "#595e7a", eye: "#dfe6f5" },
  { id: "antigravity", label: "Antigravity",  body: "#959fbf", dark: "#41465e", eye: "#dfe6f5" },
];

export const human: Tool = { id: "human", label: "You", body: "#abb6ce", dark: "#595e7a", eye: "#dfe6f5", grid: HUMAN };

export const TOOL_MAP: Record<string, Tool> = Object.fromEntries(
  [...TOOLS, human].map((t) => [t.id, t]),
);

export function PixelSprite({ tool, size = 48 }: { tool: string; size?: number }) {
  const t = TOOL_MAP[tool] ?? TOOL_MAP.claude;
  const grid = t.grid ?? ROBOT;
  const fill = (c: string) => (c === "X" ? t.body : c === "o" ? t.dark : c === "e" ? t.eye : null);
  return (
    <svg width={size} height={size} viewBox="0 0 12 12" shapeRendering="crispEdges" aria-label={t.label} className="dark:invert dark:hue-rotate-180">
      {grid.flatMap((row, y) =>
        [...row].map((ch, x) => {
          const f = fill(ch);
          return f ? <rect key={`${x}-${y}`} x={x} y={y} width={1} height={1} fill={f} /> : null;
        }),
      )}
    </svg>
  );
}
