// Message body renderer: Markdown (GFM) with no raw HTML, links opened outside
// the app, code blocks with a copy button. Long words and URLs wrap instead of
// widening the bubble.
import { memo, useState } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { isTauri } from "@tauri-apps/api/core";
import { Check, Copy } from "lucide-react";
import { cn } from "@/lib/utils";

export async function openExternal(href: string) {
  if (!/^(https?:|mailto:)/i.test(href)) return;
  if (isTauri()) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(href);
  } else window.open(href, "_blank", "noopener,noreferrer");
}

export async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
}

export function CopyButton({ text, label, className }: { text: string; label: string; className?: string }) {
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={() => {
        void copyText(text);
        setDone(true);
        setTimeout(() => setDone(false), 1400);
      }}
      className={cn("inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-muted hover:text-foreground", className)}
    >
      {done ? <Check size={12} /> : <Copy size={12} />}
    </button>
  );
}

function textOf(node: unknown): string {
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) return textOf((node as { props: { children?: unknown } }).props.children);
  return "";
}

const components: Components = {
  a: ({ href, children }) => (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        if (href) void openExternal(href);
      }}
      className="break-all underline underline-offset-2"
    >
      {children}
    </a>
  ),
  pre: ({ children }) => {
    const code = textOf(children).replace(/\n$/, "");
    return (
      <div className="group/code relative my-2 rounded-lg border border-border bg-card text-card-foreground">
        <CopyButton text={code} label="Copy code" className="absolute right-1 top-1 bg-card" />
        <pre className="overflow-x-auto p-3 pr-9 font-mono text-xs leading-relaxed">{children}</pre>
      </div>
    );
  },
  code: ({ className, children }) =>
    className ? (
      <code className={className}>{children}</code>
    ) : (
      <code className="break-words rounded bg-muted px-1 py-0.5 font-mono text-[0.85em] text-foreground">{children}</code>
    ),
  p: ({ children }) => <p className="my-1 first:mt-0 last:mb-0 [overflow-wrap:anywhere]">{children}</p>,
  ul: ({ children }) => <ul className="my-1 list-disc space-y-0.5 pl-5">{children}</ul>,
  ol: ({ children }) => <ol className="my-1 list-decimal space-y-0.5 pl-5">{children}</ol>,
  h1: ({ children }) => <p className="my-1 text-base font-semibold">{children}</p>,
  h2: ({ children }) => <p className="my-1 text-[15px] font-semibold">{children}</p>,
  h3: ({ children }) => <p className="my-1 font-semibold">{children}</p>,
  blockquote: ({ children }) => <blockquote className="my-1 border-l-2 border-border pl-3 text-muted-foreground">{children}</blockquote>,
  table: ({ children }) => (
    <div className="my-2 max-w-full overflow-x-auto rounded-lg border border-border">
      <table className="w-full border-collapse text-xs">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b border-border bg-muted px-2 py-1 text-left font-medium text-foreground">{children}</th>,
  td: ({ children }) => <td className="border-t border-border px-2 py-1 text-foreground">{children}</td>,
};

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="select-text text-sm leading-relaxed">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components} skipHtml>
        {text}
      </ReactMarkdown>
    </div>
  );
});
