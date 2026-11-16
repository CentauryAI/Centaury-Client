# ACP (Agent Client Protocol) in Centaury

**Status: not implemented; spike: OK.** `spikes/acp-codex/` completed `initialize` → `session/new` → `session/prompt`
against real codex and received `stopReason=end_turn` (output below). No ACP in `client/`, `desktop/` or the server.

## References (checked 2026-11-16)
- Spec: <https://agentclientprotocol.com/protocol/v1/overview> — **protocol v1** (`protocolVersion: 1`, integer = MAJOR version).
  [initialization](https://agentclientprotocol.com/protocol/v1/initialization) ·
  [session-setup](https://agentclientprotocol.com/protocol/v1/session-setup) ·
  [prompt-turn](https://agentclientprotocol.com/protocol/v1/prompt-turn) ·
  [tool-calls/permissions](https://agentclientprotocol.com/protocol/v1/tool-calls) ·
  [transports](https://agentclientprotocol.com/protocol/v1/transports) (JSON-RPC 2.0 over stdio, one message per line).
- Official Rust crate: [`agent-client-protocol` 5.3.0](https://docs.rs/agent-client-protocol/5.3.0) (2026-11-14,
  schema `agent-client-protocol-schema` 1.18.0). Client API: `Client.v1().connect_with(transport, |cx| …)`,
  `cx.send_request(InitializeRequest::new(ProtocolVersion::V1))`, `cx.build_session_cwd()`; v2 behind feature `unstable_protocol_v2`.
- Codex adapter: [`@agentclientprotocol/codex-acp`](https://github.com/agentclientprotocol/codex-acp) 2.7.0
  (replaces the deprecated `@zed-industries/codex-acp`). `codex` 0.144.6 does **not** speak ACP natively (only `mcp-server`, `app-server`).

## Where it fits
- **The Centaury client (`centaury` and the Tauri app) is the ACP client**: it launches the agent (codex-acp, gemini, claude-agent-acp…)
  as a stdio subprocess and drives it. For ACP-capable agents it replaces hooks + injection (`commands/launch.rs`).
- It gains what MCP cannot (`commands/mcp.rs`: the MCP agent is "deaf" unless it calls `kore_wait`): the client **pushes**
  each bus message as a new `session/prompt`. And `session/new.mcpServers` lets us pass it `centaury mcp` for sending.
- **The server does not speak ACP.** ACP is 1:1 process-to-process over stdio (the HTTP transport is still an RFD). The client bridges:
  `agent_message_chunk`s aggregated per turn → message on the project bus (existing HTTP/WS); `tool_call`/`plan`/`usage_update`
  → optional status events. The server stays bus + roster; no protocol changes except, maybe, one event type.

## Minimal scope
- Client→agent: `initialize` (v1, no `fs` or `terminal`), `session/new` (absolute cwd, `mcpServers`), `session/prompt`
  (`text` blocks only), `session/cancel` (notification). `authenticate` only if the agent requires it (codex uses `~/.codex`).
- Agent→client, must handle: `session/update` (notification) and `session/request_permission` (request: reply
  `selected` with an `optionId` or `cancelled`; initial policy = deny or ask in the desktop UI).
- Reply `-32601` to anything not advertised (`fs/*`, `terminal/*`, `elicitation/create`) and ignore `_*` notifications.

## Out of scope (for now)
`fs/read_text_file`/`fs/write_text_file` and `terminal/*` (the agent uses its own sandbox), `session/load|resume|fork`,
modes/config options, images/audio, the server speaking ACP, protocol v2 (unstable), write-mode agents without UI.

## Estimated cost: 5–8 days
Assumptions: official 5.x crate in `client/`, a single agent (codex-acp) first, bus bridge reusing `config.rs`/`ws.rs`.
- 1–2 d: `acp` module in `client/` (spawn, initialize/session/prompt, map updates) + subcommand `centaury acp <agent>`.
- 1–2 d: bus↔ACP bridge (incoming message → `session/prompt`, queue if a turn is active, output → bus) and roster registration.
- 1–2 d: permissions in the Tauri app (dialog for `request_permission`) and streaming updates to the UI.
- 1–2 d: second agent (gemini/claude ACP), errors, cancellation, manual testing.

## Risks
- Unstable crate API (5.x major, recent v1/v2 rewrites); pin the exact version.
- Third-party adapters change fast: the `codex-acp` npm package pulls `@openai/codex` 0.166 with a per-platform binary
  (in the spike the optional `codex-linux-x64` was not installed; `CODEX_PATH` = system codex 0.144.6 was used and worked).
- Non-standard extensions in `_meta` (`_auth/status_update` carries email/plan: do not forward it to the bus), agent warnings
  mixed into `agent_message_chunk` (see output), reconnections visible only in `session_info_update._meta`.
- One turn per session at a time: bus messages arriving during a turn must be queued (or use the steering extension).

## Spike (`spikes/acp-codex/`, Node without global dependencies; see its `README`)
```
cd spikes/acp-codex && npm install && node spike.mjs        # 1 prompt: "Reply only: hello", cwd = empty tmp dir
```
Real output (2026-11-16, summarized; email redacted; prompt/reply text translated to English from the original run):
```
<- initialize {"protocolVersion":1,"agentInfo":{"name":"@agentclientprotocol/codex-acp","version":"2.7.0"},
   "agentCapabilities":{"loadSession":true,"promptCapabilities":{"embeddedContext":true,"image":true},
   "sessionCapabilities":{"resume","list","close","delete","fork",…},"mcpCapabilities":{"http":true,"sse":false}},
   "authMethods":[{"id":"api-key",…}]}
<- notification _auth/status_update {kind:"account",label:"ChatGPT Plus",…}
<- session/new sessionId=01a1e67c-… (models: gpt-5.6-sol[low|medium|high]…)
<- session/update session_info_update / available_commands_update
<- session/update agent_message_chunk "Warning: Skill descriptions were shortened … \n\n"
<- session/update session_info_update {_meta.codex.error:"Reconnecting... 1/5"}
<- session/update agent_message_chunk "hello"
<- session/update usage_update {"used":17541,"size":80100}
<- session/prompt stopReason=end_turn
```
No permission or fs requests during the turn (the client would have denied them). ~58 s including one reconnection.
