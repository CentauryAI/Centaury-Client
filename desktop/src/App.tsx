import { useEffect, useState } from "react";
import { toast } from "sonner";
import { MotionConfig } from "motion/react";
import { ThemeProvider } from "@/lib/theme";
import { onAuthExpired, store } from "@/lib/api";
import { usePref } from "@/lib/prefs"; // importing it also applies Reduce motion at startup
import { Login } from "@/components/kore/Login";
import { Shell } from "@/components/kore/Shell";
import { MainTitleBar } from "@/components/kore/MainTitleBar";
import { Toaster } from "@/components/ui/sonner";

function App() {
  const [authed, setAuthed] = useState(Boolean(store.token));
  const [reduceMotion] = usePref("reduceMotion");
  // any 401 on an authenticated call = the 24h session (or instance token) lapsed → back to login
  useEffect(() => {
    onAuthExpired(() => {
      store.clear();
      setAuthed(false);
      toast.error("Your session expired. Sign in again.");
    });
    return () => onAuthExpired(null);
  }, []);
  return (
    <ThemeProvider>
      {/* CSS covers transitions (index.css); this covers the JS-driven motion components */}
      <MotionConfig reducedMotion={reduceMotion ? "always" : "user"}>
      <div className="flex h-screen w-full flex-col overflow-hidden bg-ground-app text-foreground">
        <MainTitleBar />
        <div className="relative min-h-0 flex-1 overflow-hidden">
          {authed ? (
            <Shell
              onLogout={() => {
                store.clear();
                setAuthed(false);
              }}
            />
          ) : (
            <Login onReady={() => setAuthed(true)} />
          )}
        </div>
      </div>
      </MotionConfig>
      <Toaster />
    </ThemeProvider>
  );
}

export default App;
