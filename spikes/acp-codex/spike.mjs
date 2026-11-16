// Spike: minimal ACP client (protocol v1, JSON-RPC 2.0 over stdio, newline-delimited).
// initialize -> session/new (empty temp cwd) -> ONE session/prompt; prints session/update + stopReason.
// Denies every permission request; advertises no fs/terminal capabilities.
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, createWriteStream } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import readline from "node:readline";

const here = dirname(fileURLToPath(import.meta.url));
const agentCmd = process.env.ACP_AGENT ?? join(here, "node_modules/.bin/codex-acp");
const prompt = process.argv[2] ?? "Reply only: hello";
const logDir = "/tmp/centaury-demo";
mkdirSync(logDir, { recursive: true });
const cwd = mkdtempSync(join(tmpdir(), "acp-spike-"));

// CODEX_PATH: system codex by default (the bundled @openai/codex platform binary may fail to install).
const env = {
  INITIAL_AGENT_MODE: "read-only",
  NO_BROWSER: "1",
  CODEX_PATH: join(homedir(), ".local/bin/codex"),
  ...process.env,
};
delete env.ANTHROPIC_API_KEY;
const agent = spawn(agentCmd, [], { cwd, env, stdio: ["pipe", "pipe", "pipe"] });
agent.stderr.pipe(createWriteStream(join(logDir, "acp-codex-stderr.log")));
agent.on("exit", (code, sig) => console.log(`[agent exit] code=${code} signal=${sig}`));

let nextId = 0;
const pending = new Map();
const send = (msg) => agent.stdin.write(JSON.stringify({ jsonrpc: "2.0", ...msg }) + "\n");
const request = (method, params) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    console.log(`-> ${method}`);
    send({ id, method, params });
  });
const short = (v, n = 300) => { const s = JSON.stringify(v); return s.length > n ? s.slice(0, n) + "…" : s; };

let text = "";
function onUpdate({ update }) {
  const kind = update.sessionUpdate;
  if (kind === "agent_message_chunk" && update.content?.type === "text") {
    text += update.content.text;
    console.log(`<- session/update agent_message_chunk ${JSON.stringify(update.content.text)}`);
  } else {
    console.log(`<- session/update ${kind} ${short(update, 200)}`);
  }
}

function onAgentRequest({ id, method, params }) {
  console.log(`<- ${method} (agent->client) ${short(params, 200)}`);
  if (method === "session/request_permission") {
    const deny = params.options?.find((o) => o.kind === "reject_once" || o.kind === "reject_always");
    const outcome = deny ? { outcome: "selected", optionId: deny.optionId } : { outcome: "cancelled" };
    console.log(`-> permission denied ${short(outcome)}`);
    return send({ id, result: { outcome } });
  }
  // fs/*, terminal/* were not advertised: refuse anything else.
  send({ id, error: { code: -32601, message: `spike client does not support ${method}` } });
}

readline.createInterface({ input: agent.stdout }).on("line", (line) => {
  let msg;
  try { msg = JSON.parse(line); } catch { return console.log(`[non-JSON stdout] ${line}`); }
  if (msg.method && msg.id !== undefined) return onAgentRequest(msg);
  if (msg.method === "session/update") return onUpdate(msg.params);
  // Extension notifications (`_`-prefixed, e.g. _auth/status_update) may carry account data: method only.
  if (msg.method) return console.log(`<- notification ${msg.method}${msg.method.startsWith("_") ? "" : " " + short(msg.params)}`);
  const p = pending.get(msg.id);
  if (!p) return;
  pending.delete(msg.id);
  msg.error ? p.reject(new Error(short(msg.error, 600))) : p.resolve(msg.result);
});

const timer = setTimeout(() => { console.log("[timeout 180s]"); agent.kill(); process.exit(2); }, 180_000);
try {
  const init = await request("initialize", {
    protocolVersion: 1,
    clientCapabilities: { fs: { readTextFile: false, writeTextFile: false }, terminal: false },
    clientInfo: { name: "centaury-acp-spike", version: "0.0.1" },
  });
  console.log(`<- initialize ${short(init, 1200)}`);
  if (init.protocolVersion !== 1) throw new Error(`unsupported protocolVersion ${init.protocolVersion}`);
  const session = await request("session/new", { cwd, mcpServers: [] });
  console.log(`<- session/new sessionId=${session.sessionId} ${short({ ...session, sessionId: undefined }, 400)}`);
  const res = await request("session/prompt", {
    sessionId: session.sessionId,
    prompt: [{ type: "text", text: prompt }],
  });
  console.log(`<- session/prompt stopReason=${res.stopReason}`);
  console.log(`[full text] ${JSON.stringify(text)}`);
} catch (e) {
  console.log(`[error] ${e.message}`);
  process.exitCode = 1;
} finally {
  clearTimeout(timer);
  agent.stdin.end();
  agent.kill();
}
