import { Info, Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { TitleBar } from "@/components/kore/TitleBar";

// Opens (or focuses) the About/updates child window. url stays same index.html;
// main.tsx renders <About/> when ?window=about.
async function openAbout() {
  const existing = await WebviewWindow.getByLabel("about");
  if (existing) return void existing.setFocus();
  new WebviewWindow("about", {
    url: "/?window=about",
    title: "About Kore",
    width: 340,
    height: 320,
    resizable: false,
    minimizable: false,
    maximizable: false,
    decorations: false,
    center: true,
  });
}

export function MainTitleBar() {
  const { theme, setTheme } = useTheme();
  return (
    <TitleBar
      title="Kore"
      rightActions={
        <>
          <button onClick={openAbout} className="title-bar-btn mr-1" aria-label="About / updates" tabIndex={-1}>
            <Info className="h-4 w-4" />
          </button>
          <button
            onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
            className="title-bar-btn mr-0.5"
            aria-label="Toggle theme"
            tabIndex={-1}
          >
            {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
          </button>
        </>
      }
    />
  );
}
