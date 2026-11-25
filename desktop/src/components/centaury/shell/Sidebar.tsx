import { ArrowRight, Bell, type LucideIcon } from "lucide-react";
import { cn } from "../lib/cn";

export type SidebarItem = {
  label: string;
  icon: LucideIcon;
  active?: boolean;
  onClick?: () => void;
  /** Mono eyebrow printed above this item: starts a navigation group. */
  section?: string;
  /** Count pill on the right (hidden at 0). Desktop addition. */
  badge?: number;
};

const ITEM =
  "flex items-center gap-3 rounded-full px-4 py-2.5 text-sm font-medium transition-colors";

/*
 * Dashboard sidebar (236px): brand → user → pill navigation (the active one
 * is the ink block) → `footer` anchored at the bottom with mt-auto.
 *
 * Desktop adaptation of @centaury/ui's Sidebar: no router, so items are
 * buttons (onClick) instead of next/link; `section` adds the group labels the
 * web /org panel draws; `userAction` replaces the fixed bell.
 */
export default function Sidebar({
  brand,
  user,
  userAction,
  items,
  footer,
  className,
}: {
  brand: { name: string; logoSrc?: string };
  user?: { name: string; role: string; initials: string };
  /** Slot at the right of the user row (defaults to the bell). */
  userAction?: React.ReactNode;
  items: readonly SidebarItem[];
  footer?: React.ReactNode;
  className?: string;
}) {
  return (
    <aside className={cn("flex h-full min-h-0 flex-col gap-3", className)}>
      <div className="flex items-center gap-2 px-2 pt-1">
        {/* ink mask: the house logo.svg is light and would not show on white */}
        {brand.logoSrc && (
          <span
            aria-hidden
            className="aspect-[56/59] h-6 shrink-0 bg-foreground"
            style={{
              WebkitMask: `url(${brand.logoSrc}) center / contain no-repeat`,
              mask: `url(${brand.logoSrc}) center / contain no-repeat`,
            }}
          />
        )}
        <span className="font-heading text-lg font-semibold tracking-tight">{brand.name}</span>
      </div>

      {user && (
        <div className="flex items-center gap-3 px-2 py-3">
          <span className="flex h-9 w-9 items-center justify-center rounded-full bg-muted font-heading text-sm font-semibold text-foreground">
            {user.initials}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm font-medium">{user.name}</span>
            <span className="block text-xs text-muted-foreground">{user.role}</span>
          </span>
          {userAction ?? <Bell size={16} className="text-muted-foreground" />}
        </div>
      )}

      <nav className="flex min-h-0 flex-col gap-1 overflow-y-auto">
        {items.map(({ label, icon: Icon, active, onClick, section, badge }) => (
          <div key={label} className="contents">
            {section && (
              <p className="px-4 pb-1 pt-3 font-mono text-[10px] uppercase tracking-[0.14em] text-muted-foreground">
                {section}
              </p>
            )}
            <button
              type="button"
              onClick={onClick}
              aria-current={active ? "page" : undefined}
              className={cn(
                ITEM,
                active
                  ? "bg-foreground text-background hover:bg-foreground/90"
                  : "text-foreground/70 hover:bg-muted hover:text-foreground"
              )}
            >
              <Icon size={16} />
              {label}
              {badge ? (
                <span
                  data-testid="nav-badge"
                  aria-label={`${badge} unread`}
                  className={cn("ml-auto grid h-5 min-w-5 place-items-center rounded-full px-1.5 text-[11px] font-semibold", active ? "bg-background text-foreground" : "bg-foreground text-background")}
                >
                  {badge > 99 ? "99+" : badge}
                </span>
              ) : null}
            </button>
          </div>
        ))}
      </nav>

      {footer && <div className="mt-auto">{footer}</div>}
    </aside>
  );
}

/* Dark card at the foot of the sidebar (plan, notice, call to action). */
export function SidebarPromo({
  eyebrow,
  title,
  description,
  cta,
}: {
  eyebrow?: string;
  title: string;
  description: string;
  cta?: { label: string; onClick: () => void };
}) {
  return (
    <div className="band-radial rounded-3xl p-4">
      {eyebrow && (
        <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-navbar-muted-foreground">
          {eyebrow}
        </p>
      )}
      <p className="text-ink-dark mt-3 font-heading text-xl font-semibold">{title}</p>
      <p className="mt-1 text-xs leading-relaxed text-navbar-muted-foreground">{description}</p>
      {cta && (
        <button
          type="button"
          onClick={cta.onClick}
          className="mt-4 flex w-full items-center justify-between rounded-full bg-white px-4 py-2.5 text-sm font-medium text-[#2c2e3a] transition-colors hover:bg-white/90"
        >
          {cta.label}
          <ArrowRight size={14} />
        </button>
      )}
    </div>
  );
}
