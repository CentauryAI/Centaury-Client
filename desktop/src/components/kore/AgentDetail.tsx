// AgentDetail — click a roster row → this dialog. GUI equivalent of the CLI's
// `list <name>` detail card, plus the two owner-or-self write actions the CLI
// exposes: retag (PATCH /v1/instances/{name}) and, for your own instance,
// set-status (PATCH /v1/instances/self). A directed message box sends to just
// this agent (@mention → targets=[name]). Thin: every write is one api call,
// server enforces authz. Mock-safe (api short-circuits when store.demo).
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Cpu, FolderOpen, Send, Skull, Tag, User } from "lucide-react";
import { api, store, type InstanceSummary } from "@/lib/api";
import { PixelSprite } from "@/components/kore/sprites";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { toast } from "sonner";

function Field({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-muted-foreground">{icon}</span>
      <span className="w-16 shrink-0 text-muted-foreground">{label}</span>
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

  if (!agent) return null;
  const me = store.name;
  const isSelf = agent.name === me;
  const canRetag = agent.kind === "agent" && (agent.owner === me || isSelf);

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

  return (
    <Dialog open={!!agent} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {agent.kind === "agent" ? (
              <PixelSprite tool={agent.tool ?? "claude"} size={24} />
            ) : (
              <AgentAvatar seed={agent.name} size={24} className="rounded-full" />
            )}
            {agent.tag ? `${agent.tag}-` : ""}
            {agent.name}
            <span
              className={
                "ml-1 h-2 w-2 rounded-full " +
                (agent.status === "active" ? "bg-emerald-500" : "bg-muted-foreground/40")
              }
            />
          </DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-1.5">
          <Field icon={<User className="h-3.5 w-3.5" />} label="kind" value={agent.kind} />
          {agent.owner && <Field icon={<User className="h-3.5 w-3.5" />} label="owner" value={agent.owner} />}
          {agent.tool && <Field icon={<Cpu className="h-3.5 w-3.5" />} label="tool" value={agent.tool} />}
          {agent.directory && (
            <Field icon={<FolderOpen className="h-3.5 w-3.5" />} label="dir" value={agent.directory} />
          )}
          <Field icon={<Tag className="h-3.5 w-3.5" />} label="context" value={agent.status_context || "—"} />
        </div>

        {/* set-status: only your own instance (CLI: PATCH /v1/instances/self) */}
        {isSelf && (
          <div className="border-t pt-3">
            <label className="mb-1 block text-[11px] font-medium text-muted-foreground">Your status</label>
            <div className="flex gap-2">
              <Input
                value={status}
                onChange={(e) => setStatus(e.target.value)}
                placeholder={agent.status_context || "what are you doing?"}
                className="h-8 text-sm"
              />
              <Button
                size="sm"
                disabled={busy || !status.trim()}
                onClick={() => run(() => api.setStatus(status.trim()), "Status updated").then(() => setStatus(""))}
              >
                Set
              </Button>
            </div>
          </div>
        )}

        {/* retag: owner-or-self (CLI: PATCH /v1/instances/{name}) */}
        {canRetag && (
          <div className="border-t pt-3">
            <label className="mb-1 block text-[11px] font-medium text-muted-foreground">Retag (empty = clear)</label>
            <div className="flex gap-2">
              <Input
                value={tag}
                onChange={(e) => setTag(e.target.value)}
                placeholder={agent.tag ?? "group tag"}
                className="h-8 text-sm"
              />
              <Button
                size="sm"
                variant="secondary"
                disabled={busy}
                onClick={() => run(() => api.retag(agent.name, tag.trim() || null), "Retagged").then(() => setTag(""))}
              >
                Apply
              </Button>
            </div>
          </div>
        )}

        {/* kill (DU-D6): kore-client kill = server delete + tombstone + local
            SIGTERM. Server trusts any org member; confirm() is the client-side
            friction (D10's GUI analogue). Agents only. */}
        {agent.kind === "agent" && (
          <div className="border-t pt-3">
            <Button
              size="sm"
              variant="destructive"
              className="w-full"
              disabled={busy}
              onClick={() => {
                if (store.demo) return void toast.info("Demo mode — nothing to kill.");
                if (!confirm(`Kill ${agent.name}? Closes its process and connection.`)) return;
                run(
                  () =>
                    invoke<string>("kill_agent", {
                      server: store.server,
                      token: store.token,
                      project: store.project,
                      name: agent.name,
                    }),
                  `${agent.name} killed`
                ).then(onClose);
              }}
            >
              <Skull className="mr-2 h-3.5 w-3.5" /> Kill agent
            </Button>
          </div>
        )}

        {/* directed send: message just this instance (@mention → targets=[name]) */}
        {!isSelf && (
          <div className="border-t pt-3">
            <label className="mb-1 block text-[11px] font-medium text-muted-foreground">Message {agent.name}</label>
            <Textarea
              value={msg}
              onChange={(e) => setMsg(e.target.value)}
              placeholder={`Message ${agent.name}…`}
              className="min-h-[52px] resize-none text-sm"
            />
            <Button
              size="sm"
              className="mt-2 w-full"
              disabled={busy || !msg.trim()}
              onClick={() =>
                run(() => api.send(msg.trim(), [agent.name]), `Sent to ${agent.name}`).then(() => {
                  setMsg("");
                  onClose();
                })
              }
            >
              <Send className="mr-2 h-3.5 w-3.5" /> Send
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
