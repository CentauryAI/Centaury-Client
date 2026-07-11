import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, RefreshCw, RotateCcw } from "lucide-react";
import { ThemeProvider } from "@/lib/theme";
import { TitleBar } from "@/components/kore/TitleBar";
import { Button } from "@/components/ui/button";

type Phase = "idle" | "checking" | "latest" | "available" | "installing" | "ready" | "error";

export function About() {
  const [version, setVersion] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0); // 0..1; stays 0 if server omits content-length
  const [error, setError] = useState("");

  useEffect(() => {
    void getVersion().then(setVersion);
  }, []);

  // C5: real updater (stub swap owner-approved 2026-07-06). Endpoint + pubkey
  // live in tauri.conf.json; artifacts signed with the kore-updater key.
  async function checkUpdate() {
    setPhase("checking");
    setError("");
    try {
      const u = await check();
      setUpdate(u);
      setPhase(u ? "available" : "latest");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  async function install() {
    if (!update) return;
    setPhase("installing");
    setProgress(0);
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === "Started") total = ev.data.contentLength ?? 0;
        else if (ev.event === "Progress") {
          got += ev.data.chunkLength;
          if (total > 0) setProgress(got / total);
        }
      });
      setPhase("ready");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  return (
    <ThemeProvider>
      <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
        <TitleBar title="About" showMinimize={false} showMaximize={false} />
        <main className="flex flex-1 flex-col items-center justify-center gap-5 p-6">
          <div className="text-center">
            <div className="mx-auto mb-2 grid h-12 w-12 place-items-center rounded-xl bg-primary text-lg font-bold text-primary-foreground">
              K
            </div>
            <h2 className="text-xl font-bold">Kore</h2>
            <p className="text-sm text-muted-foreground">Version {version || "…"}</p>
          </div>

          {(phase === "idle" || phase === "checking" || phase === "latest" || phase === "error") && (
            <Button
              onClick={checkUpdate}
              variant="outline"
              disabled={phase === "checking"}
              className="w-48"
            >
              <RefreshCw className={`mr-2 h-4 w-4 ${phase === "checking" ? "animate-spin" : ""}`} />
              {phase === "checking" ? "Checking…" : "Check for updates"}
            </Button>
          )}

          {phase === "latest" && (
            <p className="text-sm text-muted-foreground">You're on the latest version.</p>
          )}

          {phase === "available" && update && (
            <div className="flex flex-col items-center gap-2">
              <p className="text-sm text-muted-foreground">
                Version {update.version} is available.
              </p>
              <Button onClick={install} className="w-48">
                <Download className="mr-2 h-4 w-4" />
                Download &amp; install
              </Button>
            </div>
          )}

          {phase === "installing" && (
            <div className="flex w-48 flex-col items-center gap-2">
              <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full rounded-full bg-primary transition-[width]"
                  style={{ width: `${Math.round(progress * 100)}%` }}
                />
              </div>
              <p className="text-sm text-muted-foreground">
                Downloading… {progress > 0 ? `${Math.round(progress * 100)}%` : ""}
              </p>
            </div>
          )}

          {phase === "ready" && (
            <div className="flex flex-col items-center gap-2">
              <p className="text-sm text-muted-foreground">Update installed.</p>
              <Button onClick={() => void relaunch()} className="w-48">
                <RotateCcw className="mr-2 h-4 w-4" />
                Restart now
              </Button>
            </div>
          )}

          {phase === "error" && (
            <p className="max-w-72 break-words text-center text-sm text-destructive">{error}</p>
          )}
        </main>
      </div>
    </ThemeProvider>
  );
}
