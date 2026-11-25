import { Search } from "lucide-react";
import { cn } from "../lib/cn";

/* 32px icon button: chart and table controls. */
export function IconButton({
  label,
  className,
  children,
  ...props
}: { label: string } & React.ComponentPropsWithoutRef<"button">) {
  return (
    <button
      type="button"
      aria-label={label}
      className={cn(
        "flex h-8 w-8 items-center justify-center rounded-full border border-border bg-card text-foreground/70 transition-colors hover:text-foreground",
        className
      )}
      {...props}
    >
      {children}
    </button>
  );
}

/* Pill search field for the top bar. */
export function SearchField({
  className,
  ...props
}: React.ComponentPropsWithoutRef<"input">) {
  return (
    <label
      className={cn(
        "flex min-w-[180px] flex-1 items-center gap-2 rounded-full border border-border px-4 py-2",
        className
      )}
    >
      <Search size={14} className="text-muted-foreground" />
      <input
        type="search"
        className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
        {...props}
      />
    </label>
  );
}

/* Panel header: title in ink + actions on the right. */
export function PanelHeader({
  title,
  children,
}: {
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between">
      <h2 className="text-ink font-heading text-base font-medium">{title}</h2>
      {children && <div className="flex gap-2">{children}</div>}
    </div>
  );
}

/* Status dot: the only non-slate color allowed, and only semantic. */
export function StatusDot({
  tone,
  children,
}: {
  tone: "ok" | "pending" | "idle";
  children: React.ReactNode;
}) {
  return (
    <span className="flex items-center gap-2 whitespace-nowrap">
      <span
        className={cn(
          "h-2 w-2 rounded-full",
          tone === "ok" && "bg-status-ok",
          tone === "pending" && "bg-status-pending",
          tone === "idle" && "bg-muted-foreground/40"
        )}
      />
      {children}
    </span>
  );
}

/* Pill tabs with a counter; the active one is the ink block. */
export function TabPills<T extends string>({
  tabs,
  value,
  onChange,
}: {
  tabs: readonly { label: T; count?: number }[];
  value: T;
  onChange: (tab: T) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      {tabs.map(({ label, count }) => (
        <button
          key={label}
          type="button"
          onClick={() => onChange(label)}
          aria-pressed={value === label}
          className={cn(
            "rounded-full px-3.5 py-1.5 text-xs font-medium transition-colors",
            value === label
              ? "bg-foreground text-background"
              : "border border-border text-foreground/70 hover:bg-muted"
          )}
        >
          {label} {count !== undefined && <span className="opacity-60">{count}</span>}
        </button>
      ))}
    </div>
  );
}
