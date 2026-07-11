// Overlapping circular avatars + "+N" overflow chip. Reuses the generative
// per-seed AgentAvatar (smoothui) — colorful circles, no image assets.
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { cn } from "@/lib/utils";

export function AvatarStack({
  seeds,
  total,
  max = 5,
  size = 36,
  className,
}: {
  seeds: string[];
  total?: number; // overall headcount (may exceed shown faces) → drives "+N"
  max?: number;
  size?: number;
  className?: string;
}) {
  const shown = seeds.slice(0, max);
  const extra = (total ?? seeds.length) - shown.length;
  const overlap = Math.round(size / 3);
  return (
    <div className={cn("flex items-center", className)}>
      {shown.map((s, i) => (
        <div
          key={s}
          className="rounded-full ring-2 ring-background"
          style={{ marginLeft: i ? -overlap : 0, zIndex: max - i }}
        >
          <AgentAvatar seed={s} size={size} className="rounded-full" />
        </div>
      ))}
      {extra > 0 && (
        <div
          className="grid place-items-center rounded-full bg-muted font-medium text-muted-foreground ring-2 ring-background"
          style={{ marginLeft: -overlap, width: size, height: size, fontSize: size * 0.32 }}
        >
          +{extra}
        </div>
      )}
    </div>
  );
}
