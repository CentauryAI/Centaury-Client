// Settings — six small sections behind pill tabs. Everything here is either a
// read of GET /v1/user/me or a per-device preference (kore.pref.* in
// localStorage, see lib/prefs.ts): the server has no profile-edit, password or
// preferences routes, so none are offered. Updates reuse the About window.
import { useCallback, useEffect, useState } from "react";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { getVersion } from "@tauri-apps/api/app";
import { LogOut, RefreshCw } from "lucide-react";
import { AuthField, Button, PanelHeader, TabPills } from "@/components/centaury";
import { QueryState } from "@/components/kore/QueryState";
import { openAbout } from "@/components/kore/MainTitleBar";
import { api, errorInfo, store, type ErrorKind, type UserMe } from "@/lib/api";
import { usePref, type PrefKey } from "@/lib/prefs";
import { cn } from "@/lib/utils";

const SECTIONS = [
  { label: "Account" },
  { label: "Appearance" },
  { label: "Notifications" },
  { label: "Shortcuts" },
  { label: "Connection" },
  { label: "About" },
] as const;
type Section = (typeof SECTIONS)[number]["label"];

export function Settings({ onLogout }: { onLogout: () => void }) {
  const [section, setSection] = useState<Section>("Account");
  return (
    <div className="space-y-5 p-2">
      <PanelHeader title="Settings" />
      <TabPills tabs={SECTIONS} value={section} onChange={setSection} />
      <div className="rounded-2xl border border-border bg-card p-5">
        {section === "Account" && <Account onLogout={onLogout} />}
        {section === "Appearance" && <Appearance />}
        {section === "Notifications" && <Notifications />}
        {section === "Shortcuts" && <Shortcuts />}
        {section === "Connection" && <Connection />}
        {section === "About" && <About />}
      </div>
    </div>
  );
}

function Heading({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="mb-4">
      <h3 className="font-heading text-base font-semibold tracking-tight">{title}</h3>
      {hint && <p className="mt-1 text-sm text-muted-foreground">{hint}</p>}
    </div>
  );
}

function Row({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="flex items-center justify-between gap-4 border-t border-border py-3 first:border-t-0 first:pt-0">
      <div className="min-w-0">
        <p className="text-sm font-medium">{label}</p>
        {hint && <p className="mt-0.5 text-xs text-muted-foreground">{hint}</p>}
      </div>
      <div className="shrink-0 text-sm">{children}</div>
    </div>
  );
}

function Switch({ pref, label }: { pref: PrefKey; label: string }) {
  const [on, set] = usePref(pref);
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={label}
      onClick={() => set(!on)}
      className={cn(
        "relative h-6 w-11 rounded-full border transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring",
        on ? "border-foreground bg-foreground" : "border-border bg-muted",
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 h-4.5 w-4.5 rounded-full transition-transform",
          on ? "translate-x-5 bg-background" : "bg-muted-foreground",
        )}
      />
    </button>
  );
}

function Account({ onLogout }: { onLogout: () => void }) {
  const [me, setMe] = useState<UserMe | null>(null);
  const [err, setErr] = useState<{ kind: ErrorKind; message: string } | null>(null);
  const load = useCallback(() => {
    if (!store.session) {
      setErr({ kind: "auth", message: "No active session. Sign in again." });
      return;
    }
    setErr(null);
    api.me(store.session).then(setMe).catch((e) => setErr(errorInfo(e)));
  }, []);
  useEffect(load, [load]);

  return (
    <div>
      <Heading title="Account" hint="Your profile as the server has it. It can't be edited from here." />
      {err ? (
        <QueryState state="error" kind={err.kind} message={err.message} onRetry={load} onLogin={onLogout} className="py-6" />
      ) : !me ? (
        <QueryState state="loading" message="Loading profile…" className="py-6" />
      ) : (
        <div data-testid="profile">
          <Row label="Name">{me.display_name}</Row>
          <Row label="Email">{me.email}</Row>
          <Row label="Role" hint={me.role === "admin" ? "Can manage accounts and projects." : undefined}>
            <span className="capitalize">{me.role}</span>
          </Row>
          <Row label="Organisation">{me.org_name}</Row>
          {me.plan && (
            <Row label="Plan">
              <span className="capitalize">{me.plan}</span>
            </Row>
          )}
          <Row label="Signed in as" hint={`In project ${store.project ?? "—"}`}>
            @{store.name}
          </Row>
        </div>
      )}
      <div className="mt-4 border-t border-border pt-4">
        <Button variant="outline" size="sm" onClick={onLogout}>
          <LogOut /> Sign out
        </Button>
      </div>
    </div>
  );
}

function Appearance() {
  const { theme, setTheme } = useTheme();
  const modes = ["light", "dark", "system"] as const;
  return (
    <div>
      <Heading title="Appearance" />
      <Row label="Theme" hint="System follows your desktop's light or dark setting.">
        <div role="radiogroup" aria-label="Theme" className="flex gap-1">
          {modes.map((m) => (
            <button
              key={m}
              type="button"
              role="radio"
              aria-checked={theme === m}
              onClick={() => setTheme(m)}
              className={cn(
                "rounded-full px-3.5 py-1.5 text-xs font-medium capitalize transition-colors",
                theme === m ? "bg-foreground text-background" : "border border-border text-foreground/70 hover:bg-muted",
              )}
            >
              {m}
            </button>
          ))}
        </div>
      </Row>
      <Row label="Reduce motion" hint="Turns off animations and transitions. Also on when your system asks for it.">
        <Switch pref="reduceMotion" label="Reduce motion" />
      </Row>
    </div>
  );
}

function Notifications() {
  return (
    <div>
      <Heading title="Notifications" />
      <Row label="Show a notice when a message arrives in another conversation" hint="Appears inside the app while it's open.">
        <Switch pref="notify" label="Show a notice when a message arrives in another conversation" />
      </Row>
    </div>
  );
}

const KEYS: { keys: string[]; what: string }[] = [
  { keys: ["Enter"], what: "Send the message" },
  { keys: ["Shift", "Enter"], what: "New line" },
  { keys: ["@"], what: "Mention an agent or person (narrows who receives it)" },
  { keys: ["↑", "↓"], what: "Move through the mention suggestions" },
  { keys: ["Enter"], what: "Pick the highlighted suggestion (Tab works too)" },
  { keys: ["Esc"], what: "Close suggestions or a dialog" },
];

function Shortcuts() {
  return (
    <div>
      <Heading title="Shortcuts" hint="Fixed; they can't be changed." />
      <ul>
        {KEYS.map((k) => (
          <li key={k.keys.join("+") + k.what} className="flex items-center justify-between gap-4 border-t border-border py-2.5 first:border-t-0 first:pt-0">
            <span className="text-sm">{k.what}</span>
            <span className="flex shrink-0 gap-1">
              {k.keys.map((x) => (
                <kbd key={x} className="rounded-md border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-foreground">
                  {x}
                </kbd>
              ))}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

function Connection() {
  const [server, setServer] = useState(store.server);
  return (
    <div>
      <Heading title="Connection" />
      <div className="flex items-end gap-2">
        <div className="min-w-0 flex-1">
          <AuthField
            label="Server URL"
            id="srv"
            value={server}
            onChange={(e) => setServer(e.target.value)}
            disabled={store.demo}
            className="bg-card disabled:bg-muted disabled:text-muted-foreground"
          />
        </div>
        <Button
          className="rounded-xl"
          disabled={store.demo}
          onClick={() => {
            store.server = server;
            toast.success("Server saved");
          }}
        >
          Save
        </Button>
      </div>
      {store.demo && <p className="mt-2 text-xs text-muted-foreground">The server address is read-only in this preview.</p>}
    </div>
  );
}

function About() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
  }, []);
  return (
    <div>
      <Heading title="About" />
      <Row label="Version">{version === null ? "…" : version || "Not available in the browser"}</Row>
      <Row label="Updates" hint="Opens the update window.">
        <Button
          variant="outline"
          size="sm"
          onClick={() => openAbout().catch(() => toast.info("The update window only opens in the desktop app."))}
        >
          <RefreshCw /> Check for updates
        </Button>
      </Row>
    </div>
  );
}
