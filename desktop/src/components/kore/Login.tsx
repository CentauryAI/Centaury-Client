import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { api, store, type ProjectSummary } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Card, CardContent, CardDescription, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { GradientHeading } from "@/components/ui/gradient-heading";
import { GithubSquares } from "@/components/kore/GithubSquares";
import { DEMO_NAME, DEMO_PROJECT } from "@/lib/mock";
import { toast } from "sonner";
import { Loader2, PlayCircle } from "lucide-react";

// login → pick project → become an instance (register-human). Mirrors the CLI
// human flow (client/CLAUDE.md): session token bridges login → register-human.
export function Login({ onReady }: { onReady: () => void }) {
  const [step, setStep] = useState<"login" | "project">(store.session ? "project" : "login");
  const [server, setServer] = useState(store.server);
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [project, setProject] = useState("");
  const [busy, setBusy] = useState(false);

  async function doLogin(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    try {
      store.server = server;
      const r = await api.login(email, password);
      store.session = r.session_token;
      store.org = r.org_name;
      setStep("project");
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    if (step !== "project" || !store.session) return;
    api
      .userProjects(store.session)
      .then((p) => {
        setProjects(p);
        if (p[0]) setProject(p[0].name);
      })
      .catch((e) => toast.error(String(e)));
  }, [step]);

  function enterDemo() {
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
    try {
      const r = await api.registerHuman(project, store.session);
      store.token = r.token;
      store.project = project;
      store.name = r.name; // server-assigned (account owner_name, DU-S2)
      onReady();
    } catch (err) {
      toast.error(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="absolute inset-0 grid place-items-center overflow-hidden bg-background p-6">
      <GithubSquares />
      <motion.div className="relative z-10" initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.35 }}>
        <Card className="w-[380px] border-border/60 bg-background/80 shadow-xl backdrop-blur-xl">
          <CardHeader>
            <GradientHeading size="xxl" weight="bold">
              Kore
            </GradientHeading>
            <CardDescription>
              {step === "login" ? "Sign in to your org" : "Join a project — you become an instance"}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {step === "login" ? (
              <form onSubmit={doLogin} className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="server">Server</Label>
                  <Input id="server" value={server} onChange={(e) => setServer(e.target.value)} />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="email">Email</Label>
                  <Input id="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} required />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="password">Password</Label>
                  <Input id="password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
                </div>
                <Button type="submit" className="w-full" disabled={busy}>
                  {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  Sign in
                </Button>
              </form>
            ) : (
              <form onSubmit={join} className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="project">Project</Label>
                  <Input
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
                    <div className="flex flex-wrap gap-1 pt-1">
                      {projects.map((p) => (
                        <Badge
                          key={p.name}
                          variant={p.name === project ? "default" : "secondary"}
                          className="cursor-pointer"
                          onClick={() => setProject(p.name)}
                        >
                          {p.name} · {p.instance_count}
                        </Badge>
                      ))}
                    </div>
                  )}
                </div>
                <Button type="submit" className="w-full" disabled={busy}>
                  {busy && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
                  Join
                </Button>
              </form>
            )}
            {step === "login" && (
              <>
                <div className="my-4 flex items-center gap-3 text-xs text-muted-foreground">
                  <Separator className="flex-1" /> o <Separator className="flex-1" />
                </div>
                <Button type="button" variant="outline" className="w-full" onClick={enterDemo}>
                  <PlayCircle className="mr-2 h-4 w-4" /> Probar en modo demo (sin servidor)
                </Button>
              </>
            )}
          </CardContent>
        </Card>
      </motion.div>
    </div>
  );
}
