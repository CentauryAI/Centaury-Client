// kore plugin for OpenClaw (Peter Steinberger's personal-assistant gateway
// daemon — NOT an opencode-family coding TUI). Verified against the real
// openclaw@2026.6.11 plugin SDK d.ts, not the coding-agent plugin APIs.
//
// The three-moment kore model (session-start / drain / wait) collapses to ONE
// non-gated hook here, because OpenClaw is structurally different from the
// interactive tools: NO plugin hook can START an agent turn. `before_prompt_build`
// returns appended context; `enqueueNextTurnInjection` only rides the *next*
// turn — neither can wake an idle daemon. So a `wait` long-poll (as in the
// opencode/pi plugins, where promptAsync/sendUserMessage start a turn) would buy
// zero extra delivery here while adding a config gate + the D12 loss window.
//
//   before_prompt_build -> appendContext =
//       `centaury hook openclaw session-start` (register + bootstrap, once) +
//       `centaury hook openclaw drain` (pending messages, every turn)
//
// Every turn the daemon takes (a channel inbound, a heartbeat), pending kore
// messages ride in as appended context. `before_prompt_build` is not one of the
// gated conversation hooks (before_model_resolve/agent_end/llm_*/before_agent_*),
// so no `plugins.entries.kore.hooks.allowConversationAccess` is needed.
//
// ponytail: no proactive idle wake — a kore message that arrives while the
// daemon sits idle waits for the next turn to drain it. No plugin hook can start
// a turn, so this is an API ceiling, not a shortcut. Upgrade path if proactive
// wake is ever needed: OpenClaw's cron/heartbeat seam
// (heartbeat_prompt_contribution + ctx.getCron) to trigger turns on a schedule.
import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry"
import { spawn } from "node:child_process"

const TOOL = "openclaw"

function kore(event: string): Promise<string> {
  return new Promise((resolve) => {
    try {
      const child = spawn("centaury", ["hook", TOOL, event])
      let out = ""
      child.stdout.on("data", (c: Buffer) => (out += c.toString()))
      child.on("error", () => resolve("")) // centaury missing: never break the host
      child.on("close", (code) => resolve(code === 0 ? out : ""))
    } catch {
      resolve("")
    }
  })
}

export default definePluginEntry({
  id: "kore",
  name: "kore",
  description: "Bridge kore agent-network messages into OpenClaw turns",
  register(api) {
    // Process-scoped: the kore identity is this OpenClaw instance, not a session,
    // so register + bootstrap once for the daemon's life. Ongoing traffic rides
    // `drain` every turn regardless of session boundaries.
    let bootstrapped = false
    api.on("before_prompt_build", async () => {
      const parts: string[] = []
      if (!bootstrapped) {
        bootstrapped = true
        const b = (await kore("session-start")).trim()
        if (b) parts.push(b)
      }
      const extra = (await kore("drain")).trim()
      if (extra) parts.push(extra)
      return parts.length ? { appendContext: parts.join("\n\n") } : undefined
    })
  },
})
