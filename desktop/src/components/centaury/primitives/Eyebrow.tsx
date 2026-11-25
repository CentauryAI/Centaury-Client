import { cn } from "../lib/cn";

/* Mono, uppercase label above a heading or a stat. */
export default function Eyebrow({
  children,
  className,
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <p
      className={cn(
        "font-mono text-[11px] font-medium uppercase tracking-[0.28em] text-primary",
        className
      )}
    >
      {children}
    </p>
  );
}
