import { Info } from "lucide-react";
import { IconButton } from "@/components/centaury";
import { ThemeToggle } from "@/lib/theme";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { TitleBar } from "@/components/kore/TitleBar";

// Opens (or focuses) the About/updates child window. url stays same index.html;
// main.tsx renders <About/> when ?window=about.
export async function openAbout() {
  const existing = await WebviewWindow.getByLabel("about");
  if (existing) return void existing.setFocus();
  new WebviewWindow("about", {
    url: "/?window=about",
    title: "About Centaury",
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
  return (
    <TitleBar
      title="Centaury"
      rightActions={
        <>
          <IconButton onClick={openAbout} label="About / updates" tabIndex={-1}>
            <Info size={16} />
          </IconButton>
          <ThemeToggle tabIndex={-1} />
        </>
      }
    />
  );
}
