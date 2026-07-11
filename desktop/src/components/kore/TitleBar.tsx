import { useEffect, useState, type ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Minus, Maximize2, Minimize2, X } from "lucide-react";

// Custom draggable titlebar for the frameless window (decorations:false).
// Ported from desktop-legacy title-bar.tsx (owner asked to match it).
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
    if (!showMaximize) return;
    const win = getCurrentWebviewWindow();
    win.isMaximized().then(setIsMaximized);
    const unlisten = win.onResized(async () => setIsMaximized(await win.isMaximized()));
    return () => void unlisten.then((fn) => fn());
  }, [showMaximize]);

  const win = () => getCurrentWebviewWindow();

  return (
    <div className="flex h-8 shrink-0 select-none items-center justify-between border-b border-border/40 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div
        data-tauri-drag-region
        onDoubleClick={() => showMaximize && win().toggleMaximize()}
        className="flex grow items-center gap-2 pl-3"
      >
        {title && <span className="text-sm font-medium text-muted-foreground">{title}</span>}
      </div>

      <div className="flex items-center">
        {rightActions}
        {rightActions && (showMinimize || showMaximize || showClose) && (
          <div className="mx-1 h-4 w-px bg-border/40" />
        )}
        {showMinimize && (
          <button onClick={() => win().minimize()} className="title-bar-control" aria-label="Minimize" tabIndex={-1}>
            <Minus className="h-4 w-4" />
          </button>
        )}
        {showMaximize && (
          <button
            onClick={() => win().toggleMaximize()}
            className="title-bar-control"
            aria-label={isMaximized ? "Restore" : "Maximize"}
            tabIndex={-1}
          >
            {isMaximized ? <Minimize2 className="h-4 w-4" /> : <Maximize2 className="h-4 w-4" />}
          </button>
        )}
        {showClose && (
          <button
            onClick={() => win().close()}
            className="title-bar-control hover:bg-destructive hover:text-destructive-foreground"
            aria-label="Close"
            tabIndex={-1}
          >
            <X className="h-4 w-4" />
          </button>
        )}
      </div>
    </div>
  );
}
