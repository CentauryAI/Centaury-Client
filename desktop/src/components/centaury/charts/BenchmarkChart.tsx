
import { motion } from "motion/react";
import { cn } from "../lib/cn";

/*
 * Hand-built horizontal comparison chart in the /client house style: square
 * corners, monochrome slate ramp, ghost-gradient fills, the "our" bar solid
 * `foreground`. No chart libs, no hue (design-system §10). Data-driven — swap
 * the array, the markup never changes. Bars grow in on scroll into view.
 */
const EASE = [0.22, 1, 0.36, 1] as const;

export type BenchmarkRow = { label: string; value: number; highlight?: boolean };

export default function BenchmarkChart({
  title,
  unit = "",
  data,
  caption,
  className,
}: {
  title: string;
  unit?: string;
  data: readonly BenchmarkRow[];
  caption?: string;
  className?: string;
}) {
  const max = Math.max(...data.map((d) => d.value));

  return (
    <div
      className={cn(
        "rounded-2xl border border-border bg-[#f0f2f7] p-5 sm:p-7",
        className
      )}
    >
      <div className="flex items-baseline justify-between gap-4">
        <h3 className="text-ink font-heading text-base font-medium">{title}</h3>
        {caption && (
          /* not shrink-0: an unshrinkable caption pushed the card wider than the
             screen and gave the whole page a sideways scroll on phones */
          <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
            {caption}
          </span>
        )}
      </div>

      <ul className="mt-6 flex flex-col gap-4">
        {data.map((row, i) => (
          <li key={row.label} className="flex items-center gap-3 sm:gap-4">
            <span
              className={cn(
                "w-24 shrink-0 truncate text-xs sm:w-40 sm:text-sm",
                row.highlight
                  ? "font-medium text-foreground"
                  : "text-muted-foreground"
              )}
            >
              {row.label}
            </span>

            {/* track */}
            <span className="relative h-8 flex-1 bg-foreground/[0.04]">
              <motion.span
                className="absolute inset-y-0 left-0 flex items-center justify-end"
                initial={{ width: 0 }}
                whileInView={{ width: `${(row.value / max) * 100}%` }}
                viewport={{ once: true, margin: "-60px" }}
                transition={{ duration: 0.9, delay: i * 0.08, ease: EASE }}
                style={
                  row.highlight
                    ? { background: "var(--foreground)" }
                    : {
                        background:
                          "linear-gradient(90deg, color-mix(in srgb, var(--accent) 14%, transparent) 0%, color-mix(in srgb, var(--accent) 50%, transparent) 100%)",
                      }
                }
              >
                {/* dark right cap for depth (echoes the /client bar tip) */}
                {!row.highlight && (
                  <span className="h-full w-[3px] shrink-0 bg-[#3f4356]" />
                )}
              </motion.span>
            </span>

            <span
              className={cn(
                "w-11 shrink-0 text-right font-mono text-xs sm:text-sm",
                row.highlight ? "text-foreground" : "text-muted-foreground"
              )}
            >
              {row.value}
              {unit}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
