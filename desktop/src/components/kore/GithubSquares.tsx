// GitHub-contribution-graph background: a grid of small green squares at random
// intensity, a few gently twinkling. Self-contained (keyframes injected inline),
// theme-agnostic (the classic greens read on light and dark). Edges fade via a
// radial mask so the login card sits cleanly on top.
import { useMemo } from "react";

const COLS = 34;
const ROWS = 22;
// GitHub's palette (level 0 = empty).
const LEVELS = ["transparent", "#9be9a8", "#40c463", "#30a14e", "#216e39"];

export function GithubSquares() {
  const cells = useMemo(() => {
    const n = COLS * ROWS;
    return Array.from({ length: n }, () => {
      // weighted toward empty/low, like a real graph
      const r = Math.random();
      const level = r < 0.55 ? 0 : r < 0.75 ? 1 : r < 0.9 ? 2 : r < 0.97 ? 3 : 4;
      const twinkle = level > 0 && Math.random() < 0.14;
      return { level, twinkle, delay: Math.random() * 4, dur: 2.5 + Math.random() * 2.5 };
    });
  }, []);

  return (
    <div
      className="pointer-events-none absolute inset-0 grid place-items-center overflow-hidden"
      aria-hidden
    >
      <style>{`@keyframes ghsq{0%,100%{opacity:.35}50%{opacity:1}}`}</style>
      <div
        className="grid gap-[3px] [mask-image:radial-gradient(ellipse_60%_60%_at_center,transparent_18%,black_75%)]"
        style={{ gridTemplateColumns: `repeat(${COLS}, 13px)` }}
      >
        {cells.map((c, i) => (
          <div
            key={i}
            className="h-[13px] w-[13px] rounded-[2px]"
            style={{
              background: LEVELS[c.level],
              opacity: c.level === 0 ? 0.06 : 0.9,
              animation: c.twinkle ? `ghsq ${c.dur}s ease-in-out ${c.delay}s infinite` : undefined,
            }}
          />
        ))}
      </div>
    </div>
  );
}
