import { cn } from "../lib/cn";

/*
 * The "floating app card": tinted ground + a white rounded-[28px] card.
 * With `sidebar` the card becomes a 236px | 1fr grid (dashboard).
 *
 * Desktop adaptation of @centaury/ui's AppShell: the web version is a page
 * (min-h-screen, wide gutters, 1180px max). Here the shell fills the window
 * below the title bar, the sidebar stays put and only the content scrolls.
 */
export default function AppShell({
  sidebar,
  children,
  ground = "tint",
  className,
  contentClassName,
}: {
  sidebar?: React.ReactNode;
  children: React.ReactNode;
  /** `tint`: blue-gray ground (--ground-app) · `white`: plain background */
  ground?: "white" | "tint";
  className?: string;
  contentClassName?: string;
}) {
  return (
    <main
      className={cn(
        "h-full min-h-0 p-3 pt-1",
        ground === "tint" ? "bg-ground-app" : "bg-background"
      )}
    >
      <div
        className={cn(
          "grid h-full min-h-0 w-full gap-5 rounded-[28px] border border-border bg-card p-4",
          sidebar && "grid-cols-[236px_minmax(0,1fr)]",
          className
        )}
      >
        {sidebar}
        <div className={cn("min-h-0 min-w-0 overflow-y-auto", contentClassName)}>{children}</div>
      </div>
    </main>
  );
}
