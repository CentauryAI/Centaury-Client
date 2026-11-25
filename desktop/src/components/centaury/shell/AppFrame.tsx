import { cn } from "../lib/cn";

/*
 * Window frame for showing a product's interface inside a marketing page:
 * title bar (three dots, mono title, optional tag) over live HTML content.
 * Use `tag` to mark example content ("Example", "demo").
 */
export default function AppFrame({
  title,
  tag,
  children,
  className,
}: {
  title: string;
  tag?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("overflow-hidden rounded-2xl border border-border bg-card", className)}>
      <div className="flex items-center gap-3 border-b border-border bg-muted px-4 py-2.5">
        <span aria-hidden className="flex gap-1.5">
          <span className="h-2 w-2 rounded-full bg-foreground/15" />
          <span className="h-2 w-2 rounded-full bg-foreground/15" />
          <span className="h-2 w-2 rounded-full bg-foreground/15" />
        </span>
        <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">{title}</span>
        {tag && (
          <span className="shrink-0 rounded-full border border-border bg-card px-2 py-0.5 text-[10px] text-muted-foreground">
            {tag}
          </span>
        )}
      </div>
      {children}
    </div>
  );
}
