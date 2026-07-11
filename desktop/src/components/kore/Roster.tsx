import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { api, type InstanceSummary } from "@/lib/api";
import AgentAvatar from "@/components/smoothui/agent-avatar";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import { Cpu, Folder, User } from "lucide-react";
import { cn } from "@/lib/utils";

// Presence detail — one card per instance (avatar, status, tool, dir, owner,
// what-I'm-doing). Reads GET /v1/instances (mock in demo).
export function Roster() {
  const [roster, setRoster] = useState<InstanceSummary[]>([]);
  useEffect(() => {
    const load = () => api.instances().then(setRoster).catch((e) => toast.error(String(e)));
    load();
    const iv = setInterval(load, 5000);
    return () => clearInterval(iv);
  }, []);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-center justify-between border-b px-6 py-4">
        <h1 className="text-lg font-semibold">Roster</h1>
        <Badge variant="secondary">{roster.filter((i) => i.status === "active").length} active</Badge>
      </header>
      <div className="grid flex-1 auto-rows-min grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-3 overflow-y-auto p-6">
        {roster.map((i, idx) => (
          <motion.div key={i.name} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: idx * 0.03 }}>
            <Card className="flex gap-3 p-4">
              <div className="relative shrink-0">
                <AgentAvatar seed={i.name} size={44} animated={i.status === "active"} className="rounded-full" />
                <span
                  className={cn(
                    "absolute -bottom-0.5 -right-0.5 h-3 w-3 rounded-full border-2 border-card",
                    i.status === "active" ? "bg-emerald-500" : "bg-muted-foreground/40",
                  )}
                />
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5">
                  <span className="truncate font-medium">{i.tag ? `${i.tag}-` : ""}{i.name}</span>
                  <Badge variant={i.kind === "human" ? "default" : "outline"} className="text-[10px]">{i.kind}</Badge>
                </div>
                <p className="mt-0.5 truncate text-xs text-muted-foreground">{i.status_context || "—"}</p>
                <div className="mt-2 flex flex-wrap gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
                  {i.tool && <span className="inline-flex items-center gap-1"><Cpu className="h-3 w-3" />{i.tool}</span>}
                  {i.owner && <span className="inline-flex items-center gap-1"><User className="h-3 w-3" />{i.owner}</span>}
                  {i.directory && <span className="inline-flex items-center gap-1 truncate"><Folder className="h-3 w-3" />{i.directory}</span>}
                </div>
              </div>
            </Card>
          </motion.div>
        ))}
      </div>
    </div>
  );
}
