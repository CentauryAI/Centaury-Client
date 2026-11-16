# ACP (Agent Client Protocol) en Centaury

**Estado: no implementado; spike: OK.** `spikes/acp-codex/` completó `initialize` → `session/new` → `session/prompt`
contra codex real y recibió `stopReason=end_turn` (salida abajo). Nada de ACP en `client/`, `desktop/` ni en el server.

## Referencias (consultadas 2026-11-16)
- Spec: <https://agentclientprotocol.com/protocol/v1/overview> — **protocolo v1** (`protocolVersion: 1`, entero = versión MAJOR).
  [initialization](https://agentclientprotocol.com/protocol/v1/initialization) ·
  [session-setup](https://agentclientprotocol.com/protocol/v1/session-setup) ·
  [prompt-turn](https://agentclientprotocol.com/protocol/v1/prompt-turn) ·
  [tool-calls/permisos](https://agentclientprotocol.com/protocol/v1/tool-calls) ·
  [transports](https://agentclientprotocol.com/protocol/v1/transports) (JSON-RPC 2.0 por stdio, un mensaje por línea).
- Crate Rust oficial: [`agent-client-protocol` 5.3.0](https://docs.rs/agent-client-protocol/5.3.0) (14-11-2026,
  schema `agent-client-protocol-schema` 1.18.0). API cliente: `Client.v1().connect_with(transport, |cx| …)`,
  `cx.send_request(InitializeRequest::new(ProtocolVersion::V1))`, `cx.build_session_cwd()`; v2 tras feature `unstable_protocol_v2`.
- Adaptador codex: [`@agentclientprotocol/codex-acp`](https://github.com/agentclientprotocol/codex-acp) 2.7.0
  (sustituye a `@zed-industries/codex-acp`, deprecado). `codex` 0.144.6 **no** habla ACP nativo (solo `mcp-server`, `app-server`).

## Dónde encaja
- **El cliente Centaury (`centaury` y la app Tauri) es el cliente ACP**: lanza el agente (codex-acp, gemini, claude-agent-acp…)
  como subproceso stdio y lo conduce. Sustituye, para agentes con ACP, a hooks + inyección (`commands/launch.rs`).
- Gana lo que MCP no puede (`commands/mcp.rs`: el agente MCP es «sordo» si no llama a `kore_wait`): el cliente **empuja**
  cada mensaje del bus como un `session/prompt` nuevo. Y `session/new.mcpServers` permite pasarle `centaury mcp` para enviar.
- **El server no habla ACP.** ACP es 1:1 proceso-a-proceso por stdio (el transporte HTTP aún es RFD). El cliente hace de puente:
  `agent_message_chunk` agregados por turno → mensaje al bus del proyecto (HTTP/WS existente); `tool_call`/`plan`/`usage_update`
  → eventos de estado opcionales. El server sigue siendo bus + roster; sin cambios de protocolo salvo, quizá, un tipo de evento.

## Alcance mínimo
- Cliente→agente: `initialize` (v1, sin `fs` ni `terminal`), `session/new` (cwd absoluto, `mcpServers`), `session/prompt`
  (solo bloques `text`), `session/cancel` (notificación). `authenticate` solo si el agente lo exige (codex usa `~/.codex`).
- Agente→cliente a atender sí o sí: `session/update` (notificación) y `session/request_permission` (petición: responder
  `selected` con un `optionId` o `cancelled`; política inicial = rechazar o preguntar en la UI de escritorio).
- Responder `-32601` a lo no anunciado (`fs/*`, `terminal/*`, `elicitation/create`) e ignorar notificaciones `_*`.

## Descartado (por ahora)
`fs/read_text_file`/`fs/write_text_file` y `terminal/*` (el agente usa su propio sandbox), `session/load|resume|fork`,
modos/config options, imágenes/audio, que el server hable ACP, protocolo v2 (inestable), agentes en modo escritura sin UI.

## Coste estimado: 5–8 días
Supuestos: crate oficial 5.x en `client/`, un solo agente (codex-acp) primero, puente al bus reutilizando `config.rs`/`ws.rs`.
- 1–2 d: módulo `acp` en `client/` (spawn, initialize/session/prompt, mapear updates) + subcomando `centaury acp <agente>`.
- 1–2 d: puente bus↔ACP (mensaje entrante → `session/prompt`, cola si hay turno activo, salida → bus) y registro en roster.
- 1–2 d: permisos en la app Tauri (diálogo para `request_permission`) y streaming de updates a la UI.
- 1–2 d: segundo agente (gemini/claude ACP), errores, cancelación, pruebas manuales.

## Riesgos
- API del crate inestable (5.x mayor, reescrituras recientes v1/v2); fijar versión exacta.
- Adaptadores de terceros cambian rápido: el npm `codex-acp` arrastra `@openai/codex` 0.166 con binario por plataforma
  (en el spike no se instaló el opcional `codex-linux-x64`; se usó `CODEX_PATH` = codex 0.144.6 del sistema y funcionó).
- Extensiones no estándar en `_meta` (`_auth/status_update` trae email/plan: no reenviarlo al bus), avisos del agente
  mezclados en `agent_message_chunk` (ver salida), reconexiones visibles solo en `session_info_update._meta`.
- Un turno por sesión a la vez: mensajes del bus durante un turno hay que encolarlos (o usar la extensión de steering).

## Spike (`spikes/acp-codex/`, Node sin dependencias globales; ver su `README`)
```
cd spikes/acp-codex && npm install && node spike.mjs        # 1 prompt: «Responde solo: hola», cwd = tmp vacío
```
Salida real (2026-11-16, resumida; email redactado):
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
<- session/update agent_message_chunk "hola"
<- session/update usage_update {"used":17541,"size":80100}
<- session/prompt stopReason=end_turn
```
Sin peticiones de permiso ni de fs durante el turno (el cliente las habría rechazado). ~58 s incluyendo una reconexión.
