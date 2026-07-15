// kore plugin for OpenCode (and Kilo Code — install rewrites TOOL + the
// import below). SaaS-lean port of legacy/src/opencode_plugin/kore.ts: the
// server owns cursors/replay/identity, so no local DB, notify server, ack
// bookkeeping or reconcile timers — the plugin only bridges three moments:
//
//   session.created  -> `centaury hook TOOL session-start` (register,
//                       bootstrap text cached, injected with the 1st message)
//   chat.message     -> `centaury hook TOOL drain` (pending messages ride
//                       along as an extra part, claude-parity prompt drain)
//   idle             -> `centaury hook TOOL wait` (blocks server-side up
//                       to KORE_HOOK_TIMEOUT; any messages re-enter the agent
//                       via promptAsync — the legacy delivery loop, minus the
//                       machinery)
//
// Event shapes (properties.info.id, properties.status.type) mirror the
// legacy plugin, which ran against current OpenCode builds.
import type { Plugin } from "@opencode-ai/plugin"

const TOOL = "opencode"

export const KorePlugin: Plugin = async ({ client, $ }) => {
  let bootstrap: string | null = null
  let waiting = false // one wait-loop at a time; concurrent idles are no-ops
  // HC7 (concept from hcom d077931): `hook wait` acks after it PRINTS (D12
  // emit-then-ack), so the server will NOT replay these messages — if
  // promptAsync then rejects, the text in hand is the only copy. Stash it and
  // ride it into the next injection (next idle wait or next user turn).
  let stashed: string | null = null

  async function kore(event: string): Promise<string> {
    try {
      const r = await $.nothrow()`centaury hook ${TOOL} ${event}`.quiet()
      return r.exitCode === 0 ? r.text() : ""
    } catch {
      return "" // centaury missing/unreachable: never break the host tool
    }
  }

  async function deliverOnIdle(sid: string) {
    if (waiting) return
    waiting = true
    try {
      // Re-arm on empty returns (hcom 97c8304 concept, kore-shaped): `wait`
      // returns "" on KORE_HOOK_TIMEOUT with no traffic AND on centaury
      // errors — without a loop the agent goes deaf until the next user
      // prompt. Backoff keeps a dead server from becoming a spawn storm.
      // ponytail: sid can go stale if the user switches sessions; the failed
      // promptAsync stashes and the next idle rides it in.
      for (;;) {
        const fresh = (await kore("wait")).trim()
        const msgs = [stashed, fresh].filter(Boolean).join("\n\n")
        if (msgs) {
          stashed = null
          try {
            // Same call shape as the legacy plugin — SDK typings lag the async
            // prompt variant, body matches the sync endpoint. parts only, no
            // model payload: the session keeps its own model/variant (upstream
            // f2aa147 bug class can't happen here).
            await client.session.promptAsync({
              path: { id: sid },
              body: { parts: [{ type: "text", text: msgs }] },
            } as any)
          } catch {
            stashed = msgs // already acked server-side — retry from memory
          }
          return // injection starts a turn; its idle re-arms the next cycle
        }
        await new Promise((r) => setTimeout(r, 5000))
      }
    } catch {
      // wait itself failed: nothing was read, server still has everything
    } finally {
      waiting = false
    }
  }

  return {
    event: async ({ event }: any) => {
      try {
        if (event.type === "session.created") {
          bootstrap = (await kore("session-start")).trim() || null
        } else if (
          event.type === "session.idle" ||
          (event.type === "session.status" && event.properties?.status?.type === "idle")
        ) {
          const sid = event.properties?.sessionID ?? event.properties?.info?.id
          if (sid) void deliverOnIdle(sid)
        }
      } catch {}
    },

    "chat.message": async (_input: any, output: any) => {
      try {
        const extra = (await kore("drain")).trim()
        const inject = [bootstrap, stashed, extra].filter(Boolean).join("\n\n")
        if (!inject) return
        bootstrap = null // bootstrap rides with the first user message only
        // HC15 (opencode ≥1.17): hand-made parts fail schema validation
        // (id/sessionID/messageID became required keys) — append to an
        // existing, already-valid text part instead of pushing a new one.
        const part = Array.isArray(output?.parts)
          ? output.parts.find((p: any) => p?.type === "text" && typeof p.text === "string")
          : null
        if (part) {
          part.text += "\n\n" + inject
          stashed = null // rode along with this turn
        } else {
          stashed = inject // no text part this turn — ride the next injection
        }
      } catch {}
    },
  }
}
