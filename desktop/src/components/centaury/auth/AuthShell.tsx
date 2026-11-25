import { cn } from "../lib/cn";

/*
 * Split auth card: gray ground with a grid + floating white card.
 * Left: the form (`children`, centered, max. 360px). Right: `aside`
 * (usually <QuotePanel>), hidden below lg.
 *
 * Desktop adaptation of @centaury/ui's AuthShell: fills the window below the
 * title bar instead of the page (no min-h-screen), tighter gutters.
 */
export default function AuthShell({
  children,
  aside,
  className,
}: {
  children: React.ReactNode;
  aside?: React.ReactNode;
  className?: string;
}) {
  return (
    <main className="ground-grid h-full min-h-0 p-3 pt-1 sm:p-5 sm:pt-2">
      <div
        className={cn(
          "mx-auto grid h-full min-h-0 w-full max-w-[1180px] gap-4 rounded-[28px] bg-card p-4 shadow-[0_24px_80px_-40px_rgba(44,46,58,0.3)]",
          aside && "lg:grid-cols-2",
          className
        )}
      >
        <div className="flex min-h-0 items-center justify-center overflow-y-auto px-2 py-6 sm:px-8">
          <div className="w-full max-w-[360px]">{children}</div>
        </div>
        {aside}
      </div>
    </main>
  );
}
