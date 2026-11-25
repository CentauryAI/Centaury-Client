import { useEffect, useState, type ReactNode } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { IconButton } from "@/components/centaury";
import { Minus, Maximize2, Minimize2, X } from "lucide-react";

// Custom draggable titlebar for the frameless window (decorations:false).
// Ported from desktop-legacy title-bar.tsx (owner asked to match it).
// Outside Tauri (`pnpm dev` opened in a browser, used to preview the UI in demo
// mode) there is no window to control: the buttons are not drawn.
const inTauri = isTauri();

interface TitleBarProps {
  title?: string;
  showMinimize?: boolean;
  showMaximize?: boolean;
  showClose?: boolean;
  rightActions?: ReactNode;
}

export function TitleBar({
  title,
  showMinimize = true,
  showMaximize = true,
  showClose = true,
  rightActions,
}: TitleBarProps) {
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    if (!showMaximize || !inTauri) return;
    const win = getCurrentWebviewWindow();
    win.isMaximized().then(setIsMaximized);
    const unlisten = win.onResized(async () => setIsMaximized(await win.isMaximized()));
    return () => void unlisten.then((fn) => fn());
  }, [showMaximize]);

  const win = () => getCurrentWebviewWindow();

  return (
    <div className="flex h-10 shrink-0 select-none items-center justify-between bg-ground-app px-3">
      <div
        data-tauri-drag-region
        onDoubleClick={() => showMaximize && inTauri && win().toggleMaximize()}
        className="flex h-full grow items-center pl-1"
      >
        {title && <span data-tauri-drag-region className="font-heading text-sm font-semibold tracking-tight text-foreground">{title}</span>}
      </div>

      <div className="flex items-center gap-1">
        {rightActions}
        {rightActions && inTauri && (showMinimize || showMaximize || showClose) && (
          <div className="mx-1 h-4 w-px bg-foreground/15" />
        )}
        {inTauri && showMinimize && (
          <IconButton onClick={() => win().minimize()} label="Minimize" tabIndex={-1}>
            <Minus className="h-4 w-4" />
          </IconButton>
        )}
        {inTauri && showMaximize && (
          <IconButton
            onClick={() => win().toggleMaximize()}
            label={isMaximized ? "Restore" : "Maximize"}
            tabIndex={-1}
          >
            {isMaximized ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}
          </IconButton>
        )}
        {inTauri && showClose && (
          <IconButton
            onClick={() => win().close()}
            label="Close"
            tabIndex={-1}
          >
            <X className="h-4 w-4" />
          </IconButton>
        )}
      </div>
    </div>
  );
}
