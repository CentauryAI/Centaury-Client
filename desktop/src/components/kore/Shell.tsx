import { useState } from "react";
import { Building2, Home as HomeIcon, LogOut, Network as NetworkIcon, Settings as SettingsIcon } from "lucide-react";
import logo from "@/assets/centaury/logo.svg?no-inline";
import { AppShell, Sidebar } from "@/components/centaury";
import { api, errorInfo, store } from "@/lib/api";
import { mockOrg } from "@/lib/mock";
import { useUnreadTotal } from "@/lib/unread";
import { Home, type OpenError } from "@/components/kore/Home";
import { Network } from "@/components/kore/Network";
import { OrgAdmin } from "@/components/kore/OrgAdmin";
import { ProjectWorkspace } from "@/components/kore/ProjectWorkspace";
import { Settings } from "@/components/kore/Settings";

type View = "home" | "network" | "org" | "settings";
const NAV: { id: View; icon: typeof HomeIcon; label: string; section?: string }[] = [
  { id: "home", icon: HomeIcon, label: "Home", section: "Workspace" },
  { id: "network", icon: NetworkIcon, label: "Network" },
  { id: "org", icon: Building2, label: "Org", section: "Management" },
  { id: "settings", icon: SettingsIcon, label: "Settings" },
];

export function Shell({ onLogout }: { onLogout: () => void }) {
  const [view, setView] = useState<View>("home");
  // `project` = the project whose workspace is mounted (kept alive while you look at
  // Home/Settings so its feed keeps counting unread); `inWorkspace` = it is on screen.
  const [project, setProject] = useState<string | null>(null);
  const [inWorkspace, setInWorkspace] = useState(false);
  const unread = useUnreadTotal();
  const [opening, setOpening] = useState<string | null>(null);
  const [openError, setOpenError] = useState<OpenError | null>(null);

  function go(v: View) {
    setInWorkspace(false);
    setView(v);
  }

  // Open a project = make ITS instance token the active one first (one token per
  // project, kept side by side), then mount the workspace. If anything fails the
  // workspace is not opened, so a project's data is never painted under another's title.
  async function openProject(name: string) {
    if (opening) return;
    setOpenError(null);
    if (store.demo) {
      setProject(name);
      setInWorkspace(true);
      return;
    }
    setOpening(name);
    try {
      const have = store.tokenFor(name);
      if (have) {
        store.activate(name, have);
      } else {
        if (!store.session) throw new Error("no session");
        const r = await api.registerHuman(name, store.session);
        store.activate(name, r.token, r.name);
      }
      setProject(name);
      setInWorkspace(true);
    } catch (e) {
      // 401 also fires the global auth-expired handler (→ login); 403 = not a member
      setOpenError({ project: name, ...errorInfo(e) });
    } finally {
      setOpening(null);
    }
  }

  const initials = (store.name ?? "?").slice(0, 2).toUpperCase();
  const orgName = store.demo ? mockOrg.name : store.org ?? "Your organization";

  return (
    <AppShell
      className="max-md:grid-cols-1 max-md:auto-rows-max max-md:overflow-y-auto"
      contentClassName="max-md:min-h-[65vh]"
      sidebar={
        <Sidebar
          className="min-w-0 [&>div:nth-child(2)]:rounded-2xl [&>div:nth-child(2)]:border [&>div:nth-child(2)]:border-border [&>div:nth-child(2)]:px-3"
          brand={{ name: "Centaury", logoSrc: logo }}
          user={{ name: `@${store.name}`, role: orgName, initials }}
          userAction={false}
          items={NAV.map(({ id, ...item }) => ({
            ...item,
            active: view === id && !inWorkspace,
            onClick: () => go(id),
            badge: id === "home" && !inWorkspace ? unread : 0,
          }))}
          footer={
            <div className="space-y-2">
              <div className="rounded-2xl bg-muted p-4">
                <p className="font-mono text-[11px] uppercase tracking-[0.28em] text-primary">
                  {store.demo ? "Demo organisation" : "Organisation"}
                </p>
                <p className="mt-2 truncate font-heading text-xl font-semibold tracking-tight" title={orgName}>
                  {orgName}
                </p>
              </div>
              <button
                type="button"
                onClick={onLogout}
                className="flex w-full items-center gap-3 rounded-full px-4 py-2.5 text-sm font-medium text-foreground/75 transition-colors hover:bg-muted hover:text-foreground"
              >
                <LogOut size={16} /> Sign out
              </button>
            </div>
          }
        />
      }
    >
      {project && (
        <div className={inWorkspace ? "h-full" : "hidden"}>
          <ProjectWorkspace key={project} project={project} visible={inWorkspace} onBack={() => setInWorkspace(false)} onLogin={onLogout} />
        </div>
      )}
      {!inWorkspace &&
        (view === "home" ? (
          <Home onOpenProject={openProject} opening={opening} openError={openError} onDismissError={() => setOpenError(null)} onLogin={onLogout} />
        ) : view === "network" ? (
          <Network onLogin={onLogout} onOpenProject={openProject} />
        ) : view === "org" ? (
          <OrgAdmin onLogin={onLogout} />
        ) : (
          <Settings onLogout={onLogout} />
        ))}
    </AppShell>
  );
}
