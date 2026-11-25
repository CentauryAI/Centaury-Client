
/* Hand-built charts in the house language (design-system.md §8):
   monochrome slate, square bars, 3-layer bar anatomy, dark tooltip pill.
   Identity is never color-alone — every multi-series chart carries a
   legend + direct labels (validated: lightness-only ramp passes CVD ΔE≥12). */

import { useState } from "react";
import { cn } from "../lib/cn";

export const AMBER = "#f59e0b"; // semantic status only (design-system.md §2)

function TooltipPill({ children }: { children: React.ReactNode }) {
  return (
    <span className="pointer-events-none absolute -top-1 left-1/2 z-10 -translate-x-1/2 -translate-y-full whitespace-nowrap rounded-md bg-foreground px-2 py-0.5 text-[10px] font-medium text-background">
      {children}
    </span>
  );
}

/* ── Vertical bars, house anatomy (cap + sliver + ghost body) ── */

export function BarChart({
  data,
  selected,
  onSelect,
  height = 190,
  format = (v: number) => String(v),
}: {
  data: { label: string; value: number }[];
  selected?: number | null;
  onSelect?: (i: number) => void;
  height?: number;
  format?: (v: number) => string;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...data.map((d) => d.value));
  const active = hover ?? selected ?? null;

  return (
    <div className="relative">
      {[
        [format(max), "top-0"],
        ...(max >= 2 ? [[format(Math.round(max / 2)), "top-1/2"]] : []),
        ["0", "bottom-6"],
      ].map(([label, pos], i) => (
        <div
          key={i}
          className={cn("absolute inset-x-0 flex items-center gap-2", pos)}
        >
          <span className="h-px flex-1 bg-foreground/[0.07]" />
          <span className="w-8 text-right text-[10px] text-muted-foreground">
            {label}
          </span>
        </div>
      ))}

      <div className="flex items-end gap-[3px] pr-10" style={{ height }}>
        {data.map((d, i) => {
          const isActive = active === i;
          return (
            <button
              key={d.label}
              type="button"
              onClick={() => onSelect?.(i)}
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover(null)}
              aria-label={`${d.label}: ${format(d.value)}`}
              className="group relative flex h-full w-full min-w-0 max-w-24 flex-1 cursor-pointer flex-col justify-end"
            >
              {isActive && (
                <TooltipPill>
                  {d.label} · {format(d.value)}
                </TooltipPill>
              )}
              {d.value === 0 ? (
                <span className="h-[2px] w-full bg-foreground/15" />
              ) : (
                <span
                  className="flex flex-col transition-[height] duration-300"
                  style={{ height: `${(d.value / max) * 100}%` }}
                >
                  {isActive ? (
                    <span className="flex-1 bg-foreground" />
                  ) : (
                    <>
                      <span className="h-[5px] shrink-0 bg-[#3f4356]" />
                      <span className="h-[3px] shrink-0 bg-card" />
                      <span
                        className="flex-1"
                        style={{
                          background: `linear-gradient(180deg, color-mix(in srgb, var(--accent) ${
                            i % 2 ? 50 : 26
                          }%, transparent) 0%, color-mix(in srgb, var(--accent) 2%, transparent) 100%)`,
                        }}
                      />
                    </>
                  )}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div className="mt-2 flex gap-[3px] pr-10">
        {data.map((d) => (
          <span
            key={d.label}
            className="w-full min-w-0 max-w-24 flex-1 truncate text-center text-[10px] text-muted-foreground"
          >
            {d.label}
          </span>
        ))}
      </div>
    </div>
  );
}

/* ── Stacked day bars (activity + amber status segment) ─────── */

export function DayBars({
  days,
  height = 120,
  legend = ["Activity", "Failed logins"],
}: {
  days: { label: string; base: number; alert: number }[];
  height?: number;
  /** Legend labels: [base segment, alert segment] */
  legend?: readonly [string, string];
}) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(1, ...days.map((d) => d.base + d.alert));

  return (
    <div>
      <div className="flex items-end gap-[3px]" style={{ height }}>
        {days.map((d, i) => {
          const total = d.base + d.alert;
          return (
            <div
              key={d.label}
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover(null)}
              className="relative flex h-full flex-1 flex-col justify-end"
            >
              {hover === i && (
                <TooltipPill>
                  {d.label} · {d.base} events
                  {d.alert > 0 && ` · ${d.alert} failed`}
                </TooltipPill>
              )}
              <span
                className="flex flex-col justify-end gap-[2px]"
                style={{ height: `${Math.max(total > 0 ? 4 : 0, (total / max) * 100)}%` }}
              >
                {d.alert > 0 && (
                  <span
                    className="w-full shrink-0"
                    style={{
                      height: `${(d.alert / Math.max(total, 1)) * 100}%`,
                      minHeight: 3,
                      background: AMBER,
                    }}
                  />
                )}
                {d.base > 0 && (
                  <span
                    className="w-full flex-1"
                    style={{
                      background:
                        hover === i
                          ? "#2c2e3a"
                          : `linear-gradient(180deg, color-mix(in srgb, var(--accent) 50%, transparent) 0%, color-mix(in srgb, var(--accent) 8%, transparent) 100%)`,
                    }}
                  />
                )}
              </span>
            </div>
          );
        })}
      </div>
      <div className="mt-2 flex justify-between text-[10px] text-muted-foreground">
        <span>{days[0]?.label}</span>
        <span>{days[days.length - 1]?.label}</span>
      </div>
      <ul className="mt-3 flex items-center gap-4 text-[11px] text-muted-foreground">
        <li className="flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-[3px] bg-accent" /> {legend[0]}
        </li>
        <li className="flex items-center gap-1.5">
          <span className="h-2 w-2 rounded-[3px]" style={{ background: AMBER }} />{" "}
          {legend[1]}
        </li>
      </ul>
    </div>
  );
}

/* ── Cumulative growth line (SVG, primary stroke + ghost area) ── */

export function GrowthLine({
  points,
  height = 150,
}: {
  points: { label: string; value: number }[];
  height?: number;
}) {
  const [hover, setHover] = useState<number | null>(null);
  const W = 320;
  const H = 110;
  const n = points.length;
  if (n === 0) {
    return (
      <p className="py-8 text-center text-sm text-muted-foreground">No data yet</p>
    );
  }
  const max = Math.max(1, ...points.map((p) => p.value));
  const x = (i: number) => (n === 1 ? W / 2 : (i / (n - 1)) * W);
  const y = (v: number) => H - (v / max) * (H - 10);
  const path = points.map((p, i) => `${i ? "L" : "M"}${x(i)},${y(p.value)}`).join(" ");
  const last = points[n - 1];

  return (
    <div className="relative" style={{ height }}>
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="h-full w-full"
        aria-hidden
      >
        <defs>
          <linearGradient id="growth-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.22" />
            <stop offset="100%" stopColor="var(--accent)" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path d={`${path} L${W},${H} L0,${H} Z`} fill="url(#growth-fill)" />
        <path
          d={path}
          fill="none"
          stroke="var(--accent)"
          strokeWidth={2}
          vectorEffect="non-scaling-stroke"
        />
        {hover !== null && (
          <line
            x1={x(hover)}
            x2={x(hover)}
            y1={0}
            y2={H}
            stroke="#2c2e3a"
            strokeOpacity={0.25}
            strokeWidth={1}
            vectorEffect="non-scaling-stroke"
          />
        )}
      </svg>

      {/* hover hit columns + marker dots (HTML so they don't distort) */}
      <div className="absolute inset-0 flex">
        {points.map((p, i) => (
          <div
            key={i}
            onMouseEnter={() => setHover(i)}
            onMouseLeave={() => setHover(null)}
            className="relative h-full flex-1"
          >
            {hover === i && (
              <span
                className="absolute left-1/2 -translate-x-1/2"
                style={{ top: `${(y(p.value) / H) * 100}%` }}
              >
                <span className="absolute -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white bg-accent"
                  style={{ width: 10, height: 10 }}
                />
                <TooltipPill>
                  {p.label} · {p.value}
                </TooltipPill>
              </span>
            )}
          </div>
        ))}
      </div>

      <span className="absolute bottom-1 right-0 text-[10px] font-medium text-muted-foreground">
        {last.label} · {last.value}
      </span>
    </div>
  );
}

/* ── Quota meter ───────────────────────────────────────────── */

export function Meter({ value, max }: { value: number; max: number }) {
  const pct = Math.min(100, (value / Math.max(1, max)) * 100);
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-foreground/10"
      role="meter"
      aria-valuenow={value}
      aria-valuemin={0}
      aria-valuemax={max}
    >
      <span
        className="block h-full rounded-full transition-[width] duration-500"
        style={{
          width: `${pct}%`,
          background: pct >= 90 ? AMBER : "#2c2e3a",
        }}
      />
    </div>
  );
}
