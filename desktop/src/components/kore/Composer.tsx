// The one composer of a project chat. States who the message reaches before you
// send it ("Everyone in <project> · N agents" / "Only to atlas, nova"); an @name
// in the text makes it directed (the server decides the scope from mentions, the
// UI only mirrors that). @ autocomplete is keyboard-driven over the roster.
import { useEffect, useMemo, useRef, useState } from "react";
import { CornerUpLeft, Send, X } from "lucide-react";
import type { InstanceSummary } from "@/lib/api";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

export type ComposerMode = { kind: "global" } | { kind: "dm"; peer: string } | { kind: "thread"; name: string };
export type ReplyTarget = { id: number; from: string; snippet: string };

export function Composer({ roster, me, project, mode, replyTo, onCancelReply, onSend, seed }: {
  roster: InstanceSummary[];
  me: string;
  project: string;
  mode: ComposerMode;
  replyTo?: ReplyTarget | null;
  onCancelReply?: () => void;
  onSend: (text: string, targets: string[]) => boolean;
  /** set the draft from outside (e.g. "@atlas " when replying); `n` changes → applies */
  seed?: { text: string; n: number };
}) {
  const [draft, setDraft] = useState("");
  const [caret, setCaret] = useState(0);
  const [pick, setPick] = useState(0);
  const [dismissed, setDismissed] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (!seed?.n) return;
    setDraft((d) => (d.trim() ? d : seed.text));
    setCaret(seed.text.length);
    requestAnimationFrame(() => ref.current?.focus());
  }, [seed?.n]); // eslint-disable-line react-hooks/exhaustive-deps
  const names = useMemo(() => roster.filter((i) => i.name !== me), [roster, me]);
  const nameSet = useMemo(() => new Set(names.map((i) => i.name)), [names]);

  // @token under the caret → suggestions
  const token = /(^|\s)@([\w.-]*)$/.exec(draft.slice(0, caret));
  const query = token ? token[2].toLowerCase() : null;
  const options = useMemo(
    () => (query === null ? [] : names.filter((i) => i.name.toLowerCase().includes(query)).sort((a, b) => Number(b.name.toLowerCase().startsWith(query)) - Number(a.name.toLowerCase().startsWith(query))).slice(0, 6)),
    [names, query],
  );
  const open = options.length > 0 && !dismissed;

  const mentions = useMemo(() => {
    const out: string[] = [];
    for (const m of draft.matchAll(/(?:^|\s)@([\w.-]+)/g)) if (nameSet.has(m[1]) && !out.includes(m[1])) out.push(m[1]);
    return out;
  }, [draft, nameSet]);
  const targets = mode.kind === "dm" ? [mode.peer] : mentions;
  const agents = names.filter((i) => i.kind === "agent").length;
  const audience =
    targets.length > 0
      ? `Only to ${targets.join(", ")}`
      : `Everyone in ${project} · ${agents} agent${agents === 1 ? "" : "s"}`;

  function choose(i: InstanceSummary) {
    if (!token) return;
    const start = caret - token[0].length + token[1].length;
    const next = `${draft.slice(0, start)}@${i.name} ${draft.slice(caret)}`;
    const pos = start + i.name.length + 2;
    setDraft(next);
    setCaret(pos);
    setPick(0);
    requestAnimationFrame(() => {
      ref.current?.focus();
      ref.current?.setSelectionRange(pos, pos);
    });
  }

  function submit() {
    if (onSend(draft, targets)) {
      setDraft("");
      setCaret(0);
    }
  }

  const placeholder =
    mode.kind === "dm" ? `Message ${mode.peer}` : mode.kind === "thread" ? `Reply in ${mode.name}` : `Message ${project}`;
  const has = draft.trim().length > 0;

  return (
    <div className="border-t border-border px-3 pb-2.5 pt-2">
      {replyTo && (
        <div className="mb-1.5 flex items-center gap-2 rounded-lg bg-muted px-2.5 py-1 text-xs text-muted-foreground">
          <CornerUpLeft size={12} className="shrink-0" />
          <span className="min-w-0 flex-1 truncate">
            Replying to <span className="font-medium text-foreground">{replyTo.from}</span>: {replyTo.snippet}
          </span>
          <button type="button" aria-label="Cancel reply" onClick={onCancelReply} className="shrink-0 rounded p-0.5 hover:bg-card">
            <X size={12} />
          </button>
        </div>
      )}
      <div className="relative">
        {open && (
          <ul role="listbox" aria-label="Mention suggestions" className="absolute bottom-full left-0 z-20 mb-1 w-64 overflow-hidden rounded-xl border border-border bg-card py-1 shadow-lg">
            {options.map((i, k) => (
              <li
                key={i.name}
                role="option"
                aria-selected={k === pick}
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(i);
                }}
                onMouseEnter={() => setPick(k)}
                className={cn("flex cursor-pointer items-baseline gap-2 px-3 py-1.5 text-sm", k === pick && "bg-muted")}
              >
                <span className="font-medium">{i.name}</span>
                {i.tag && <span className="text-xs text-muted-foreground">{i.tag}</span>}
                <span className="ml-auto text-xs text-muted-foreground">{i.kind === "agent" ? i.tool ?? "agent" : "person"}</span>
              </li>
            ))}
          </ul>
        )}
        <div className="flex items-end gap-2">
          <Textarea
            ref={ref}
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              setCaret(e.target.selectionStart ?? e.target.value.length);
              setPick(0);
              setDismissed(false);
            }}
            onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? 0)}
            onKeyDown={(e) => {
              if (open) {
                if (e.key === "ArrowDown") { e.preventDefault(); setPick((p) => (p + 1) % options.length); return; }
                if (e.key === "ArrowUp") { e.preventDefault(); setPick((p) => (p - 1 + options.length) % options.length); return; }
                if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); choose(options[pick]); return; }
                if (e.key === "Escape") { e.preventDefault(); setDismissed(true); return; }
              }
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                submit();
              }
            }}
            placeholder={placeholder}
            aria-label="Message"
            className="max-h-32 min-h-[40px] flex-1 resize-none rounded-xl border-border bg-card py-2 text-sm focus:border-primary"
            rows={1}
          />
          <button
            type="button"
            onClick={submit}
            disabled={!has}
            aria-label="Send"
            className={cn(
              "flex h-9 w-9 shrink-0 items-center justify-center rounded-full transition-colors",
              has ? "bg-foreground text-background hover:opacity-90" : "bg-muted text-muted-foreground",
            )}
          >
            <Send className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      <div className="mt-1 flex items-center justify-between gap-3 text-[11px] text-muted-foreground">
        <span data-testid="audience" className={cn("min-w-0 truncate", targets.length > 0 && "font-medium text-foreground")}>{audience}</span>
        <span className="shrink-0">Shift+Enter for a new line</span>
      </div>
    </div>
  );
}
