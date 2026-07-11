// kore extension for Pi Coding Agent (and oh-my-pi — install rewrites the
// import below). SaaS-lean port of hcom's pi_plugin/hcom.ts: the server owns
// cursors/replay/identity, so no local DB, notify server, ack bookkeeping or
// reconcile timers — the extension only bridges three moments, exactly like
// the kore opencode plugin:
//
//   before_agent_start -> `kore-client hook pi session-start` (register,
//                         bootstrap injected once as a hidden message)
//   tool_result        -> `kore-client hook pi drain` (mid-turn messages ride
//                         in as a follow-up, claude-parity)
//   agent_end (idle)   -> `kore-client hook pi wait` (blocks server-side up
//                         to KORE_HOOK_TIMEOUT; messages re-enter via
//                         sendUserMessage)
//
// Pi's ExtensionAPI: pi.on(event, handler); pi.sendUserMessage(text[, opts]);
// a before_agent_start handler may return a hidden bootstrap message.
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent"

const TOOL = "pi"

export default function koreExtension(pi: ExtensionAPI) {
  let bootstrap: string | null = null
  let bootstrapInjectedFor: string | null = null
  let waiting = false // one wait-loop at a time; concurrent idles are no-ops
  // HC7 loss window: `hook wait`/`drain` ack after they PRINT (D12), so the
  // server won't replay these — if sendUserMessage throws, the text in hand is
  // the only copy. Stash it and ride it into the next delivery.
  let stashed: string | null = null

  async function kore(event: string): Promise<string> {
    try {
      const child = require("node:child_process").spawn("kore-client", ["hook", TOOL, event])
      let out = ""
      child.stdout.on("data", (c: Buffer) => (out += c.toString()))
      return await new Promise<string>((resolve) => {
        child.on("error", () => resolve(""))
        child.on("close", (code: number) => resolve(code === 0 ? out : ""))
      })
    } catch {
      return "" // kore-client missing/unreachable: never break the host tool
    }
  }

  function inject(ctx: ExtensionContext, text: string) {
    try {
      if (ctx.isIdle()) {
        pi.sendUserMessage(text)
      } else {
        pi.sendUserMessage(text, { deliverAs: "followUp" })
      }
    } catch {
      stashed = text // already acked server-side — retry from memory next time
    }
  }

  pi.on("before_agent_start", async (_event, ctx) => {
    if (bootstrap === null) {
      bootstrap = (await kore("session-start")).trim() || null
    }
    const sid = ctx.sessionManager.getSessionId()
    if (!bootstrap || bootstrapInjectedFor === sid) return undefined
    bootstrapInjectedFor = sid
    return { message: { customType: "kore-bootstrap", content: bootstrap, display: false } }
  })

  pi.on("tool_result", async (_event, ctx) => {
    const extra = (await kore("drain")).trim()
    const text = [stashed, extra].filter(Boolean).join("\n\n")
    if (text) {
      stashed = null
      inject(ctx, text)
    }
  })

  pi.on("turn_end", async (_event, ctx) => {
    if (stashed) {
      const text = stashed
      stashed = null
      inject(ctx, text)
    }
  })

  pi.on("agent_end", async (_event, ctx) => {
    if (waiting || !ctx.isIdle?.()) return
    waiting = true
    try {
      // Re-arm on empty returns (hcom 97c8304 concept, kore-shaped): `wait`
      // returns "" on KORE_HOOK_TIMEOUT with no traffic AND on kore-client
      // errors — without a loop the agent goes deaf until the next human
      // prompt. Backoff keeps a dead server from becoming a spawn storm.
      // ponytail: loop holds this handler's promise for the session's life;
      // stale-session ceiling = the isIdle guard below.
      for (;;) {
        const fresh = (await kore("wait")).trim()
        const text = [stashed, fresh].filter(Boolean).join("\n\n")
        if (text) {
          stashed = null
          inject(ctx, text)
          return // injection starts a turn; its agent_end re-arms the next cycle
        }
        if (!ctx.isIdle?.()) return // user turn in flight — its agent_end re-arms
        await new Promise((r) => setTimeout(r, 5000))
      }
    } finally {
      waiting = false
    }
  })
}
