import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { Download, RefreshCw, RotateCcw } from "lucide-react";
import { ThemeProvider } from "@/lib/theme";
import { TitleBar } from "@/components/kore/TitleBar";
import { AppShell, Button, Meter } from "@/components/centaury";
import logo from "@/assets/centaury/logo.svg?no-inline";

type Phase = "idle" | "checking" | "latest" | "available" | "installing" | "ready" | "error";
// Bounded so a dead/slow endpoint can't leave the button spinning forever.
const CHECK_TIMEOUT_MS = 15_000;

export function About() {
  const [version, setVersion] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0); // 0..1; stays 0 if server omits content-length
  const [error, setError] = useState<{ msg: string; detail: string } | null>(null);

  useEffect(() => {
    void getVersion().then(setVersion);
  }, []);

  // C5: real updater (stub swap owner-approved 2026-07-06). Endpoint + pubkey
  // live in tauri.conf.json; artifacts signed with the kore-updater key.
  // Missing endpoint (404), network down or malformed latest.json all reject
  // here → friendly message, button re-enabled for a retry.
  async function checkUpdate() {
    setPhase("checking");
    setError(null);
    try {
      const u = await check({ timeout: CHECK_TIMEOUT_MS });
      setUpdate(u);
      setPhase(u ? "available" : "latest");
    } catch (e) {
      console.warn("update check failed:", e);
      setError({ msg: "Couldn't check for updates. Try again later.", detail: String(e) });
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
      console.warn("update install failed:", e);
      setError({ msg: "Couldn't install the update.", detail: String(e) });
      setPhase("error");
    }
  }

  return (
    <ThemeProvider>
      <div className="flex h-screen w-full flex-col overflow-hidden bg-ground-app text-foreground">
        <TitleBar title="About Centaury" showMinimize={false} showMaximize={false} />
        <div className="min-h-0 flex-1">
          <AppShell contentClassName="flex flex-col items-center justify-center-safe gap-4 text-center">
            <div>
              <span
                aria-hidden
                className="mx-auto mb-2 block aspect-[56/59] h-10 bg-foreground"
                style={{ mask: `url(${logo}) center / contain no-repeat` }}
              />
              <h1 className="text-ink font-heading text-2xl font-semibold tracking-tight">Centaury</h1>
              <p className="mt-1 text-sm text-muted-foreground">Version {version || "…"}</p>
            </div>

            {(phase === "idle" || phase === "checking" || phase === "latest" || phase === "error") && (
              <Button
                onClick={checkUpdate}
                disabled={phase === "checking"}
                className="w-48"
              >
                <RefreshCw className={phase === "checking" ? "animate-spin" : undefined} />
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
                  <Download size={16} />
                  Download &amp; install
                </Button>
              </div>
            )}

            {phase === "installing" && (
              <div className="flex w-48 flex-col items-center gap-2">
                <div className="w-full [&_[role=meter]>span]:bg-foreground!">
                  <Meter value={Math.round(progress * 100)} max={100} />
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
                  <RotateCcw size={16} />
                  Restart now
                </Button>
              </div>
            )}

            {phase === "error" && error && (
              <div className="w-full rounded-xl bg-muted px-3 py-2">
                <p className="text-sm text-foreground">{error.msg}</p>
                <p className="mt-1 text-xs break-all text-muted-foreground">{error.detail}</p>
              </div>
            )}
          </AppShell>
        </div>
      </div>
    </ThemeProvider>
  );
}
