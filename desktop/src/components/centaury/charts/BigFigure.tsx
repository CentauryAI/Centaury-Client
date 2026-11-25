import { ArrowUpRight } from "lucide-react";

/* Large chart-header figure, with a dimmed prefix and a delta. */
export default function BigFigure({
  value,
  prefix,
  delta,
}: {
  value: string;
  prefix?: string;
  delta?: string;
}) {
  return (
    <p className="flex items-center gap-2">
      <span className="font-heading text-3xl font-semibold tracking-tight tabular-nums">
        {prefix && <span className="text-muted-foreground/60">{prefix}</span>}
        {value}
      </span>
      {delta && (
        <span className="flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-[11px] font-medium text-muted-foreground">
          {delta} <ArrowUpRight size={11} />
        </span>
      )}
    </p>
  );
}
