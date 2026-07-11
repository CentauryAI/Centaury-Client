import { useState } from "react";
import { ThemeProvider } from "@/lib/theme";
import { store } from "@/lib/api";
import { Login } from "@/components/kore/Login";
import { Shell } from "@/components/kore/Shell";
import { MainTitleBar } from "@/components/kore/MainTitleBar";
import { Toaster } from "@/components/ui/sonner";

function App() {
  const [authed, setAuthed] = useState(Boolean(store.token));
  return (
    <ThemeProvider>
      <div className="flex h-screen w-screen flex-col overflow-hidden">
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
      <Toaster />
    </ThemeProvider>
  );
}

export default App;
