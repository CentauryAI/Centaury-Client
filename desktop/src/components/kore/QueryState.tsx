// The three non-data states of a fetched view, kept distinct on purpose:
// loading (spinner), empty (nothing there — not an error), error (what went
// wrong + retry). Never render an exception as an empty screen.
import { AlertTriangle, Loader2, Lock } from "lucide-react";
import Button from "@/components/centaury/primitives/Button";
import { cn } from "@/lib/utils";
import type { ErrorKind } from "@/lib/api";

export function QueryState({ state, message, kind, onRetry, onLogin, className }: {
  state: "loading" | "empty" | "error";
  message?: string;
  kind?: ErrorKind;
  onRetry?: () => void;
  onLogin?: () => void;
  className?: string;
}) {
  return (
    <div
      role={state === "error" ? "alert" : "status"}
      className={cn("flex flex-col items-center justify-center gap-2 px-6 py-10 text-center text-sm text-muted-foreground", className)}
    >
      {state === "loading" && <Loader2 size={18} className="animate-spin" />}
      {state === "error" && (kind === "forbidden" ? <Lock size={18} /> : <AlertTriangle size={18} className="text-destructive" />)}
      <p className={cn(state === "error" && "text-foreground")}>{message ?? (state === "loading" ? "Loading…" : "Nothing here yet.")}</p>
      {state === "error" && (
        <div className="mt-1 flex gap-2">
          {kind === "auth" && onLogin && (
            <Button size="sm" variant="primary" onClick={onLogin}>Sign in</Button>
          )}
          {kind !== "forbidden" && onRetry && (
            <Button size="sm" variant="outline" onClick={onRetry}>Retry</Button>
          )}
        </div>
      )}
    </div>
  );
}
