import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { ArrowLeft, Loader2, PlayCircle, Server } from "lucide-react";
import handsImg from "@/assets/centaury/hands.webp";
import {
  AuthDivider,
  AuthField,
  AuthNotice,
  AuthShell,
  AuthSubmit,
  PasswordField,
  QuotePanel,
  SocialButton,
  cn,
  type Quote,
} from "@/components/centaury";
import { ApiError, api, errorInfo, store, type ProjectSummary } from "@/lib/api";
import { DEMO_NAME, DEMO_PROJECT } from "@/lib/mock";

/* Real product framing — copied verbatim from Centaury web /login. */
const QUOTES: readonly Quote[] = [
  {
    text: "“Centaury is the interconnection layer for AI agents — the SMTP of AI. Agents from every provider talk to each other durably, with isolation and audit built in.”",
    name: "The thesis",
    role: "What we build",
    org: "Centaury",
  },
  {
    text: "“Every message is committed before it's delivered. Offline agents replay what they missed on reconnect — nothing is lost, ever.”",
    name: "Durable messaging",
    role: "At-least-once delivery",
    org: "Centaury",
  },
  {
    text: "“Multi-tenant by design — Postgres Row-Level Security fails closed. No org can see another org's agents or messages.”",
    name: "Isolation",
    role: "Enterprise-ready",
    org: "Centaury",
  },
];

const EASE = [0.16, 1, 0.3, 1] as const;

function formatError(ex: unknown): string {
  return errorInfo(ex).message;
}

// login → pick project → become an instance (register-human). Mirrors the CLI
// human flow (client/CLAUDE.md): session token bridges login → register-human.
export function Login({ onReady }: { onReady: () => void }) {
  const [step, setStep] = useState<"login" | "project">(store.session ? "project" : "login");
  const [server, setServer] = useState(store.server);
  const [editingServer, setEditingServer] = useState(false);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [project, setProject] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function doLogin(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      store.server = server;
      const r = await api.login(email, password);
      store.session = r.session_token;
      store.org = r.org_name;
      setStep("project");
    } catch (ex) {
      setErr(ex instanceof ApiError && ex.status === 401 ? "Wrong email or password." : formatError(ex));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (step !== "project" || !store.session) return;
    let cancelled = false;
    api
      .userProjects(store.session)
      .then((p) => {
        if (cancelled) return;
        setProjects(p);
        if (p[0]) setProject((cur) => cur || p[0].name);
      })
      .catch((ex) => {
        if (!cancelled) setErr(formatError(ex));
      });
    return () => {
      cancelled = true;
    };
  }, [step]);

  function enterDemo() {
    setErr(null);
    store.demo = true;
    store.token = "demo";
    store.name = DEMO_NAME;
    store.project = DEMO_PROJECT;
    onReady();
  }

  async function join(e: React.FormEvent) {
    e.preventDefault();
    if (!store.session) return;
    setBusy(true);
    setErr(null);
    try {
      const r = await api.registerHuman(project, store.session);
      store.activate(project, r.token, r.name); // name is server-assigned (account owner_name, DU-S2)
      onReady();
    } catch (ex) {
      setErr(formatError(ex));
    } finally {
      setBusy(false);
    }
  }

  function resetAccount() {
    store.session = null;
    store.org = null;
    setProjects([]);
    setProject("");
    setErr(null);
    setStep("login");
  }

  const texture = (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 select-none overflow-hidden bg-card"
    >
      <div className="absolute inset-x-4 top-6 h-[56%]">
        <img
          src={handsImg}
          alt=""
          className="h-full w-full object-contain object-center opacity-35 mix-blend-multiply [mask-image:radial-gradient(ellipse_74%_66%_at_50%_50%,black_20%,transparent_70%)] dark:opacity-20 dark:invert dark:mix-blend-screen"
        />
      </div>
    </div>
  );

  return (
    <AuthShell
      aside={
        <QuotePanel
          quotes={QUOTES}
          texture={texture}
          prevLabel="Previous quote"
          nextLabel="Next quote"
        />
      }
    >
      <motion.div
        key={step}
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.35, ease: EASE }}
      >
        {step === "login" ? (
          <>
            <h1 className="text-ink text-center font-heading text-3xl font-semibold tracking-tight">
              Welcome back
            </h1>
            <p className="mt-2 text-center text-sm text-muted-foreground">
              Please enter your details to log in.
            </p>

            <form onSubmit={doLogin} className="mt-7 flex flex-col gap-4">
              <AuthField
                label="Email address"
                id="email"
                type="email"
                required
                autoComplete="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="Enter your email address"
              />

              <PasswordField
                label="Password"
                id="password"
                required
                autoComplete="current-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter your password"
              />

              {err && <AuthNotice tone="warn">{err}</AuthNotice>}

              <AuthSubmit disabled={busy} className="flex items-center justify-center gap-2">
                {busy && <Loader2 size={15} className="animate-spin" />}
                {busy ? "Signing in…" : "Log In"}
              </AuthSubmit>
            </form>

            <AuthDivider label="or" />

            <SocialButton
              icon={<PlayCircle size={16} />}
              className="w-full"
              onClick={enterDemo}
            >
              Try demo mode (no server)
            </SocialButton>

            <div className="mt-6">
              {editingServer ? (
                <AuthField
                  label="Server URL"
                  id="server"
                  type="url"
                  value={server}
                  onChange={(e) => setServer(e.target.value)}
                  placeholder="http://localhost:8080"
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      setEditingServer(false);
                    }
                  }}
                  hint={
                    <button
                      type="button"
                      onClick={() => setEditingServer(false)}
                      className="text-xs text-muted-foreground hover:text-foreground"
                    >
                      Done
                    </button>
                  }
                />
              ) : (
                <div className="flex items-center justify-center gap-1.5 text-xs text-muted-foreground">
                  <Server size={12} className="shrink-0 opacity-75" />
                  <span>Server:</span>
                  <span className="max-w-[190px] truncate">{server}</span>
                  <button
                    type="button"
                    onClick={() => setEditingServer(true)}
                    className="font-medium text-foreground underline underline-offset-4 transition-opacity hover:opacity-80"
                  >
                    Change
                  </button>
                </div>
              )}
            </div>
          </>
        ) : (
          <>
            <h1 className="text-ink text-center font-heading text-3xl font-semibold tracking-tight">
              Pick a project
            </h1>
            <p className="mt-2 text-center text-sm text-muted-foreground">
              {store.org
                ? `Signed in to ${store.org}. Join a project as an instance.`
                : "Join a project to connect as an instance."}
            </p>

            <form onSubmit={join} className="mt-7 flex flex-col gap-4">
              <AuthField
                label="Project"
                id="project"
                list="projects"
                value={project}
                onChange={(e) => setProject(e.target.value)}
                placeholder="default"
                required
              />
              <datalist id="projects">
                {projects.map((p) => (
                  <option key={p.name} value={p.name} />
                ))}
              </datalist>

              {projects.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {projects.map((p) => (
                    <button
                      key={p.name}
                      type="button"
                      onClick={() => setProject(p.name)}
                      className={cn(
                        "inline-flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium transition-colors",
                        p.name === project
                          ? "bg-foreground text-background"
                          : "border border-border bg-card text-muted-foreground hover:bg-muted hover:text-foreground"
                      )}
                    >
                      <span>{p.name}</span>
                      <span className="font-mono opacity-70">{p.instance_count}</span>
                    </button>
                  ))}
                </div>
              )}

              {err && <AuthNotice tone="warn">{err}</AuthNotice>}

              <AuthSubmit disabled={busy} className="flex items-center justify-center gap-2">
                {busy && <Loader2 size={15} className="animate-spin" />}
                {busy ? "Joining…" : "Join project"}
              </AuthSubmit>
            </form>

            <div className="mt-6 text-center">
              <button
                type="button"
                onClick={resetAccount}
                className="inline-flex items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
              >
                <ArrowLeft size={12} />
                Back to sign in
              </button>
            </div>
          </>
        )}
      </motion.div>
    </AuthShell>
  );
}
