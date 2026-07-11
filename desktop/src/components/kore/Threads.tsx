import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { toast } from "sonner";
import { Hash } from "lucide-react";

// Named threads in the bubble (GET /v1/threads; mock in demo).
export function Threads() {
  const [threads, setThreads] = useState<{ name: string; message_count: number; last_msg_id: number }[]>([]);
  useEffect(() => {
    api.threads().then(setThreads).catch((e) => toast.error(String(e)));
  }, []);

  return (
    <div className="flex h-full flex-col">
      <header className="border-b px-6 py-4">
        <h1 className="text-lg font-semibold">Threads</h1>
      </header>
      <div className="flex flex-1 flex-col gap-2 overflow-y-auto p-6">
        {threads.length === 0 && <p className="text-sm text-muted-foreground">No threads yet.</p>}
        {threads.map((t) => (
          <Card key={t.name} className="flex items-center gap-3 p-3">
            <div className="grid h-9 w-9 place-items-center rounded-lg bg-muted"><Hash className="h-4 w-4" /></div>
            <span className="flex-1 truncate font-medium">{t.name}</span>
            <Badge variant="secondary">{t.message_count} msgs</Badge>
          </Card>
        ))}
      </div>
    </div>
  );
}
