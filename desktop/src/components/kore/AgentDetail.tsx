// AgentDetail — click a roster row → this dialog. GUI equivalent of the CLI's
// `list <name>` detail card, plus the two owner-or-self write actions the CLI
// exposes: retag (PATCH /v1/instances/{name}) and, for your own instance,
// set-status (PATCH /v1/instances/self). Order: state + Message first, details,
// your status / tag, and Kill last behind "More" with a confirmation dialog.
// A directed message box sends to just
// this agent (@mention → targets=[name]). Thin: every write is one api call,
// server enforces authz. Mock-safe (api short-circuits when store.demo).
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen, MoreHorizontal, Send, Skull, User } from "lucide-react";
import { api, store, type InstanceSummary } from "@/lib/api";
import { PixelSprite } from "@/components/kore/sprites";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { toast } from "sonner";
import { StatusDot } from "@/components/centaury";
import Button from "@/components/centaury/primitives/Button";

function Field({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-muted-foreground">{icon}</span>
      <span className="w-16 shrink-0 font-mono text-muted-foreground">{label}</span>
      <span className="truncate font-medium">{value}</span>
    </div>
  );
}

export function AgentDetail({
  agent,
  onClose,
  onChanged,
}: {
  agent: InstanceSummary | null;
  onClose: () => void;
  onChanged: () => void;
}) {
  const [tag, setTag] = useState("");
  const [status, setStatus] = useState("");
  const [msg, setMsg] = useState("");
  const [busy, setBusy] = useState(false);
  const [confirmKill, setConfirmKill] = useState(false);

  if (!agent) return null;
  const me = store.name;
  const isSelf = agent.name === me;
  const canRetag = agent.kind === "agent" && (agent.owner === me || isSelf);
  const live = agent.status === "active";

  async function run(fn: () => Promise<unknown>, ok: string) {
    setBusy(true);
    try {
      await fn();
      toast.success(ok);
      onChanged();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  function kill() {
    if (!agent) return;
    if (store.demo) {
      setConfirmKill(false);
      return void toast.info("Demo mode — nothing to kill.");
    }
    const name = agent.name;
    run(
      () => invoke<string>("kill_agent", { server: store.server, token: store.token, project: store.project, name }),
      `${name} killed`,
    ).then(() => {
      setConfirmKill(false);
      onClose();
    });
  }

  return (
    <>
      <Dialog open={!!agent} onOpenChange={(o) => !o && onClose()}>
        <DialogContent className="sm:max-w-md rounded-2xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2 font-heading text-base font-semibold">
              {agent.kind === "agent" ? (
                <PixelSprite tool={agent.tool ?? "claude"} size={24} />
              ) : (
                <AgentAvatar seed={agent.name} size={24} className="rounded-full" />
              )}
              {agent.name}
              {agent.tag && <span className="text-xs font-normal text-muted-foreground">{agent.tag}</span>}
            </DialogTitle>
            {/* §2 rule 4: presence lies — dot ≠ ready/dead, so say it in words */}
            <DialogDescription className="flex min-w-0 items-center gap-1.5">
              <StatusDot tone={live ? "ok" : "idle"}>{live ? "Connected" : "Not listening"}</StatusDot>
              <span className="min-w-0 truncate" title={agent.status_context || undefined}>
                {agent.status_context ? `· ${agent.status_context}` : ""}
              </span>
            </DialogDescription>
          </DialogHeader>

          {/* directed send: message just this instance (@mention → targets=[name]) */}
          {!isSelf && (
            <div>
              <label htmlFor="agent-msg" className="mb-1 block text-sm font-medium">Message {agent.name}</label>
              <Textarea
                id="agent-msg"
                value={msg}
                onChange={(e) => setMsg(e.target.value)}
                placeholder={`Write to ${agent.name}…`}
                className="min-h-[52px] resize-none rounded-xl text-sm focus:border-primary"
              />
              <div className="mt-2 flex justify-end">
                <Button
                  size="sm"
                  variant="primary"
                  disabled={busy || !msg.trim()}
                  onClick={() =>
                    run(() => api.send(msg.trim(), [agent.name]), `Sent to ${agent.name}`).then(() => {
                      setMsg("");
                      onClose();
                    })
                  }
                >
                  <Send /> Message
                </Button>
              </div>
            </div>
          )}

          <div className="flex flex-col gap-1.5 border-t border-border pt-3">
            <Field icon={<User className="h-3.5 w-3.5" />} label="type" value={agent.kind === "agent" ? "Agent" : "Person"} />
            {agent.owner && <Field icon={<User className="h-3.5 w-3.5" />} label="owner" value={agent.owner} />}
            {agent.tool && <Field icon={<PixelSprite tool={agent.tool} size={14} />} label="tool" value={agent.tool} />}
            {agent.directory && <Field icon={<FolderOpen className="h-3.5 w-3.5" />} label="folder" value={agent.directory} />}
          </div>

          {/* set-status: only your own instance (CLI: PATCH /v1/instances/self) */}
          {isSelf && (
            <div className="border-t border-border pt-3">
              <label htmlFor="agent-status" className="mb-1 block text-sm font-medium">Your status</label>
              <div className="flex gap-2">
                <Input
                  id="agent-status"
                  value={status}
                  onChange={(e) => setStatus(e.target.value)}
                  placeholder={agent.status_context || "what are you doing?"}
                  className="h-8 rounded-xl text-sm focus:border-primary"
                />
                <Button
                  size="sm"
                  variant="accent"
                  disabled={busy || !status.trim()}
                  onClick={() => run(() => api.setStatus(status.trim()), "Status updated").then(() => setStatus(""))}
                >
                  Set
                </Button>
              </div>
            </div>
          )}

          {/* tag: owner-or-self (CLI: PATCH /v1/instances/{name}); empty tag = none */}
          {canRetag && (
            <div className="border-t border-border pt-3">
              <label htmlFor="agent-tag" className="mb-1 block text-sm font-medium">Tag</label>
              <div className="flex gap-2">
                <Input
                  id="agent-tag"
                  value={tag}
                  onChange={(e) => setTag(e.target.value)}
                  placeholder={agent.tag ? `Now: ${agent.tag}` : "No tag yet"}
                  className="h-8 rounded-xl text-sm focus:border-primary"
                />
                <Button
                  size="sm"
                  variant="outline"
                  disabled={busy || !tag.trim()}
                  onClick={() => run(() => api.retag(agent.name, tag.trim()), "Tag updated").then(() => setTag(""))}
                >
                  Save
                </Button>
                {agent.tag && (
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={busy}
                    onClick={() => run(() => api.retag(agent.name, null), "Tag removed").then(() => setTag(""))}
                  >
                    Remove tag
                  </Button>
                )}
              </div>
            </div>
          )}

          {/* kill (DU-D6): centaury kill = server delete + tombstone + local SIGTERM.
              The server trusts any org member, so the friction lives here. */}
          {agent.kind === "agent" && (
            <div className="flex justify-end border-t border-border pt-3">
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button size="sm" variant="outline" disabled={busy}>
                    <MoreHorizontal /> More
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuItem variant="destructive" onSelect={() => setConfirmKill(true)}>
                    <Skull /> Kill agent…
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          )}
        </DialogContent>
      </Dialog>

      <Dialog open={confirmKill} onOpenChange={(o) => !busy && setConfirmKill(o)}>
        <DialogContent className="sm:max-w-sm rounded-2xl">
          <DialogHeader>
            <DialogTitle className="font-heading text-base font-semibold">Kill {agent.name}?</DialogTitle>
            <DialogDescription>
              {agent.name} will be removed from {store.project ?? "this project"} and its process on this computer will be
              stopped. This can't be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" size="sm" disabled={busy} onClick={() => setConfirmKill(false)}>
              Cancel
            </Button>
            <button
              type="button"
              disabled={busy}
              onClick={kill}
              className="inline-flex items-center justify-center gap-2 rounded-full bg-destructive px-4 py-2 text-xs font-medium text-white transition-opacity hover:opacity-90 disabled:opacity-50"
            >
              <Skull className="size-4" /> Kill {agent.name}
            </button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
