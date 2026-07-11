import { useState } from "react";
import { Building2, Home as HomeIcon, LogOut, Network as NetworkIcon, Settings as SettingsIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import { store } from "@/lib/api";
import { Home } from "@/components/kore/Home";
import { Network } from "@/components/kore/Network";
import { OrgAdmin } from "@/components/kore/OrgAdmin";
import { ProjectWorkspace } from "@/components/kore/ProjectWorkspace";
import { Settings } from "@/components/kore/Settings";
import { Badge } from "@/components/ui/badge";

type View = "home" | "network" | "org" | "settings";
const NAV: { id: View; icon: typeof HomeIcon; label: string }[] = [
  { id: "home", icon: HomeIcon, label: "Home" },
  { id: "network", icon: NetworkIcon, label: "Network" },
  { id: "org", icon: Building2, label: "Org" },
  { id: "settings", icon: SettingsIcon, label: "Settings" },
];

const KORE_ART = ` @--@--@
  |  |  |
 [==Kore==]`;

export function Shell({ onLogout }: { onLogout: () => void }) {
  const [view, setView] = useState<View>("home");
  const [project, setProject] = useState<string | null>(null);

  function go(v: View) {
    setProject(null);
    setView(v);
  }

  const initials = (store.name ?? "?").slice(0, 2).toUpperCase();

  return (
    <div className="h-full bg-ground p-3">
      <div className="flex h-full overflow-hidden rounded-[24px] border border-border bg-card text-foreground">
        {/* sidebar */}
        <aside className="flex w-60 shrink-0 flex-col gap-3 border-r bg-muted/40 p-3">
          {/* brand */}
          <div className="flex items-center gap-2 px-2 pt-1">
            <div className="grid h-7 w-7 place-items-center rounded-lg bg-primary text-sm font-bold text-primary-foreground">K</div>
            <span className="k-title text-lg">Kore</span>
          </div>

          {/* user card */}
          <div className="flex items-center gap-3 rounded-2xl border border-border bg-card p-3">
            <span className="grid h-9 w-9 place-items-center rounded-full bg-muted text-sm font-semibold">{initials}</span>
            <span className="min-w-0 flex-1 leading-tight">
              <span className="block truncate text-sm font-medium">@{store.name}</span>
              <span className="block truncate text-xs text-muted-foreground">{store.org ?? "your org"}</span>
            </span>
          </div>

          {/* nav tiles — active = dark foreground tile */}
          <nav className="flex flex-col gap-2">
            {NAV.map((n) => {
              const active = view === n.id && !project;
              return (
                <button
                  key={n.id}
                  onClick={() => go(n.id)}
                  className={cn(
                    "flex items-center gap-3 rounded-2xl px-4 py-3 text-sm font-medium transition-colors",
                    active
                      ? "bg-foreground text-background"
                      : "border border-border text-foreground/75 hover:bg-card",
                  )}
                >
                  <n.icon className="h-[18px] w-[18px]" />
                  {n.label}
                </button>
              );
            })}
          </nav>

          {/* bottom card */}
          <div className="mt-auto rounded-2xl bg-card p-4">
            <pre className="select-none font-mono text-[11px] leading-[1.35] text-primary" aria-hidden>{KORE_ART}</pre>
            <div className="mt-3 flex items-center gap-2">
              <p className="k-title flex-1 text-base">Fleet</p>
              {store.demo && <Badge variant="secondary">DEMO</Badge>}
            </div>
            <button
              onClick={onLogout}
              className="mt-3 flex w-full items-center justify-center gap-2 rounded-xl bg-foreground px-4 py-2.5 text-sm font-medium text-background transition-opacity hover:opacity-90"
            >
              <LogOut className="h-4 w-4" />
              Sign out
            </button>
          </div>
        </aside>

        <div className="min-w-0 flex-1">
          {project ? (
            <ProjectWorkspace project={project} onBack={() => setProject(null)} />
          ) : view === "home" ? (
            <Home onOpenProject={setProject} />
          ) : view === "network" ? (
            <Network />
          ) : view === "org" ? (
            <OrgAdmin />
          ) : (
            <Settings onLogout={onLogout} />
          )}
        </div>
      </div>
    </div>
  );
}
