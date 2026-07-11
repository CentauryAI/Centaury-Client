import { useState } from "react";
import { store } from "@/lib/api";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { useTheme } from "next-themes";
import { toast } from "sonner";
import { LogOut, Moon, Sun } from "lucide-react";

export function Settings({ onLogout }: { onLogout: () => void }) {
  const [server, setServer] = useState(store.server);
  const { theme, setTheme } = useTheme();

  return (
    <div className="flex h-full flex-col">
      <header className="border-b px-6 py-4">
        <h1 className="k-title text-lg">Settings</h1>
      </header>
      <div className="mx-auto flex w-full max-w-xl flex-col gap-4 p-6">
        <Card className="flex flex-col gap-3 rounded-2xl p-4">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm font-medium">Identity</div>
              <div className="text-xs text-muted-foreground">@{store.name} · {store.project}</div>
            </div>
            {store.demo && <Badge variant="secondary">DEMO</Badge>}
          </div>
        </Card>

        <Card className="flex flex-col gap-2 rounded-2xl p-4">
          <Label htmlFor="srv">Server URL</Label>
          <div className="flex gap-2">
            <Input id="srv" value={server} onChange={(e) => setServer(e.target.value)} disabled={store.demo} />
            <Button
              variant="secondary"
              disabled={store.demo}
              onClick={() => {
                store.server = server;
                toast.success("Server saved");
              }}
            >
              Save
            </Button>
          </div>
          {store.demo && <p className="text-xs text-muted-foreground">Demo mode — server disabled.</p>}
        </Card>

        <Card className="flex items-center justify-between rounded-2xl p-4">
          <div className="text-sm font-medium">Theme</div>
          <Button variant="outline" size="sm" onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
            {theme === "dark" ? <Sun className="mr-2 h-4 w-4" /> : <Moon className="mr-2 h-4 w-4" />}
            {theme === "dark" ? "Light" : "Dark"}
          </Button>
        </Card>

        <Separator />
        <Button variant="destructive" onClick={onLogout}>
          <LogOut className="mr-2 h-4 w-4" /> Sign out
        </Button>
      </div>
    </div>
  );
}
