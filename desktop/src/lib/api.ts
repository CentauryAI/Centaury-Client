// Kore server client. Thin: connections + wire types only, all logic server-side
// (SaaS split). HTTP rides tauri-plugin-http (bypasses the webview CORS wall);
// live delivery rides tauri-plugin-websocket (sets the Bearer header the server
// requires — a browser WebSocket cannot). Wire shapes mirror kore-protocol.
import { fetch } from "@tauri-apps/plugin-http";
import WebSocket from "@tauri-apps/plugin-websocket";
import * as mock from "@/lib/mock";

// ---- wire types (kore-protocol/src/api.rs, message.rs) ----
export type LoginResponse = { session_token: string; org_name: string; display_name: string };
export type OrgMemberSummary = { display_name: string; owner_name: string; role: string; disabled: boolean };
// DU-D10 (org admin) — mirror kore-protocol OrgUserSummary/OrgUsageResponse/ProjectMemberSummary
export type OrgUserSummary = { id: string; email: string; display_name: string; role: string; disabled: boolean; created_at: string };
export type OrgUsage = { agents: number; max_agents: number; humans: number; users: number; projects: { name: string; instances: number }[]; messages_30d: number };
export type ProjectMemberSummary = { user_id: string; display_name: string; owner_name: string };
export type ProjectSummary = { name: string; instance_count: number };
export type RegisterResponse = { token: string; name: string };
export type SendResponse = { id: number; scope: string; mentions: string[] };
// GET /v1/user/me (subset the app uses)
export type UserMe = { email: string; display_name: string; role: string; org_name: string; plan?: string; agents?: number; max_agents?: number };
// GET /v1/org/agents (admin = whole org, member = their projects) and GET /v1/org/audit (admin)
export type OrgAgent = { id: string; name: string; project: string; role: string | null; status: string; last_seen: string | null; messages_sent: number };
export type AuditEntry = { id: number; actor_kind: string; actor_label: string; action: string; target: string | null; ip: string | null; data: unknown; created_at: string };
export type InstanceSummary = {
  name: string;
  tag: string | null;
  status: string; // "active" | "inactive"
  kind: string; // "agent" | "human"
  owner: string | null;
  tool: string | null;
  directory: string | null;
  status_context: string;
  last_seen_msg_id: number;
};
export type Message = {
  from: string;
  sender_kind: string; // "agent" | "human" | "system"
  scope: string; // "broadcast" | "mentions" (MessageScope, lowercase on the wire)
  text: string;
  mentions: string[];
  delivered_to: string[];
  intent: string | null;
  thread: string | null;
  reply_to: number | null;
  bundle_id: string | null;
};
export type Delivery = { id: number } & Message;
// GET /v1/org/projects/{name}/messages (SESSION token): carries created_at, lacks sender_kind/delivered_to/bundle_id
export type OrgMessage = {
  id: number; from: string; scope: string; mentions: string[]; text: string;
  intent: string | null; thread: string | null; reply_to: number | null; created_at: string;
};
export type ThreadSummary = { name: string; message_count: number; last_msg_id: number };

// ---- session state (localStorage; token files are the CLI's job) ----
const K = {
  server: "kore.server",
  session: "kore.session_token",
  token: "kore.instance_token",
  project: "kore.project",
  name: "kore.name",
  org: "kore.org",
  demo: "kore.demo",
  tokens: "kore.instance_tokens", // { [project]: { token, at } } — one instance token per project
} as const;

// instance tokens live 7 days server-side; refresh a bit earlier
const TOKEN_MAX_AGE_MS = 6 * 24 * 3600 * 1000;
type TokenMap = Record<string, { token: string; at: number }>;
function readTokens(): TokenMap {
  try {
    return JSON.parse(localStorage.getItem(K.tokens) || "{}") as TokenMap;
  } catch {
    return {};
  }
}

export const store = {
  get server() {
    return localStorage.getItem(K.server) || "http://localhost:8080";
  },
  set server(v: string) {
    localStorage.setItem(K.server, v.replace(/\/$/, ""));
  },
  get session() {
    return localStorage.getItem(K.session);
  },
  set session(v: string | null) {
    v ? localStorage.setItem(K.session, v) : localStorage.removeItem(K.session);
  },
  get token() {
    return localStorage.getItem(K.token);
  },
  set token(v: string | null) {
    v ? localStorage.setItem(K.token, v) : localStorage.removeItem(K.token);
  },
  get project() {
    return localStorage.getItem(K.project);
  },
  set project(v: string | null) {
    v ? localStorage.setItem(K.project, v) : localStorage.removeItem(K.project);
  },
  get name() {
    return localStorage.getItem(K.name);
  },
  set name(v: string | null) {
    v ? localStorage.setItem(K.name, v) : localStorage.removeItem(K.name);
  },
  get org() {
    return localStorage.getItem(K.org);
  },
  set org(v: string | null) {
    v ? localStorage.setItem(K.org, v) : localStorage.removeItem(K.org);
  },
  get demo() {
    return localStorage.getItem(K.demo) === "1";
  },
  set demo(v: boolean) {
    v ? localStorage.setItem(K.demo, "1") : localStorage.removeItem(K.demo);
  },
  // the instance token for a specific project (null if none / stale)
  tokenFor(project: string): string | null {
    const t = readTokens()[project];
    return t && Date.now() - t.at < TOKEN_MAX_AGE_MS ? t.token : null;
  },
  // remember a project's token and make it the active project
  activate(project: string, token: string, name?: string) {
    localStorage.setItem(K.tokens, JSON.stringify({ ...readTokens(), [project]: { token, at: Date.now() } }));
    store.project = project;
    store.token = token;
    if (name) store.name = name;
  },
  clear() {
    [K.session, K.token, K.project, K.name, K.org, K.demo, K.tokens].forEach((k) => localStorage.removeItem(k));
  },
};

// ---- errors: one shape so every screen can tell 401/403/429/network/5xx apart ----
export type ErrorKind = "auth" | "forbidden" | "rate" | "server" | "network" | "other";
export class ApiError extends Error {
  constructor(public status: number, public detail: string) {
    super(`${status} ${detail}`.trim());
  }
}
export function errorInfo(e: unknown): { kind: ErrorKind; message: string } {
  if (e instanceof ApiError) {
    const d = e.detail && e.detail.length < 160 ? ` (${e.detail})` : "";
    if (e.status === 401) return { kind: "auth", message: "Your session expired. Sign in again." };
    if (e.status === 403) return { kind: "forbidden", message: `You don't have permission for this${d}` };
    if (e.status === 429) return { kind: "rate", message: "Too many requests. Wait a moment and retry." };
    if (e.status >= 500) return { kind: "server", message: `The server had a problem (${e.status}). Retry in a moment.` };
    return { kind: "other", message: e.detail || `Request failed (${e.status})` };
  }
  const m = e instanceof Error ? e.message : String(e);
  return { kind: "network", message: `Can't reach the server (${m}). Check your connection.` };
}

// App registers this: a 401 on any authenticated call = session/token expired → back to login.
let authExpiredHandler: (() => void) | null = null;
export function onAuthExpired(fn: (() => void) | null) {
  authExpiredHandler = fn;
}

async function req<T>(path: string, opts: { method?: string; body?: unknown; token?: string | null } = {}): Promise<T> {
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (opts.token) headers.Authorization = `Bearer ${opts.token}`;
  const res = await fetch(`${store.server}${path}`, {
    method: opts.method || "GET",
    headers,
    body: opts.body ? JSON.stringify(opts.body) : undefined,
  });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    if (res.status === 401 && opts.token && !store.demo) authExpiredHandler?.();
    throw new ApiError(res.status, detail || res.statusText);
  }
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}

export const api = {
  // auth
  login: (email: string, password: string) =>
    req<LoginResponse>("/v1/auth/login", { method: "POST", body: { email, password } }),
  // become an instance in a project (Bearer = session token). Name is
  // server-derived from the account's owner_name (DU-S2) — read it from r.name.
  registerHuman: (project: string, session: string) =>
    req<RegisterResponse>("/v1/auth/register-human", { method: "POST", body: { project }, token: session }),

  // reads (Bearer = instance token)
  userProjects: (session: string) =>
    store.demo ? Promise.resolve(mock.mockProjects) : req<ProjectSummary[]>("/v1/user/projects", { token: session }),
  // org directory (DU-S4): any member, session token — powers the owner picker
  orgMembers: (session: string) =>
    store.demo ? Promise.resolve(mock.mockMembers) : req<OrgMemberSummary[]>("/v1/org/members", { token: session }),
  // role comes from here (admin | member), not from probing /v1/org/usage
  me: (session: string) =>
    store.demo
      ? Promise.resolve<UserMe>({ email: "you@demo", display_name: "You", role: "member", org_name: mock.mockOrg.name, plan: "demo" })
      : req<UserMe>("/v1/user/me", { token: session }),
  orgAgents: (session: string) =>
    store.demo ? Promise.resolve(mock.mockOrgAgents) : req<OrgAgent[]>("/v1/org/agents", { token: session }),
  orgAudit: (session: string, limit = 50, before?: number) =>
    req<AuditEntry[]>(`/v1/org/audit?limit=${limit}${before ? `&before=${before}` : ""}`, { token: session }),
  // `token` overrides the active project's token (e.g. Network drilling into another joined project)
  instances: (token: string | null = store.token) =>
    store.demo ? Promise.resolve(mock.mockRoster) : req<InstanceSummary[]>("/v1/instances", { token }),
  history: (limit = 50, thread?: string) =>
    store.demo
      ? Promise.resolve(thread ? mock.mockFeed.filter((m) => m.thread === thread) : mock.mockFeed)
      : req<Delivery[]>(`/v1/messages?limit=${limit}${thread ? `&thread=${encodeURIComponent(thread)}` : ""}`, {
          token: store.token,
        }),
  unread: () => (store.demo ? Promise.resolve({ count: 0 }) : req<{ count: number }>("/v1/messages/unread", { token: store.token })),
  threads: () => (store.demo ? Promise.resolve<ThreadSummary[]>(mock.mockThreads) : req<ThreadSummary[]>("/v1/threads", { token: store.token })),
  // timestamps + backwards paging (the only route that has either); needs the session token
  orgMessages: (project: string, limit = 200, before?: number) =>
    store.demo
      ? Promise.resolve(mock.mockOrgMessages(limit, before))
      : req<OrgMessage[]>(
          `/v1/org/projects/${encodeURIComponent(project)}/messages?limit=${limit}${before ? `&before=${before}` : ""}`,
          { token: store.session },
        ),

  // send — in demo, no server; the caller echoes the local Delivery itself.
  send: (text: string, targets: string[] = [], reply_to?: number, thread?: string) =>
    store.demo
      ? Promise.resolve<SendResponse>({ id: mock.mockSend(text, targets, reply_to, thread).id, scope: targets.length ? "mentions" : "broadcast", mentions: targets })
      : req<SendResponse>("/v1/messages", {
          method: "POST",
          body: { text, targets, reply_to, thread },
          token: store.token,
        }),

  // set my own instance status line (PATCH /v1/instances/self)
  setStatus: (status_context: string) =>
    store.demo
      ? Promise.resolve(undefined)
      : req<void>("/v1/instances/self", { method: "PATCH", body: { status_context }, token: store.token }),

  // retag an agent (owner-or-self gated server-side; PATCH /v1/instances/{name})
  retag: (name: string, tag: string | null) =>
    store.demo
      ? Promise.resolve(undefined)
      : req<void>(`/v1/instances/${encodeURIComponent(name)}`, { method: "PATCH", body: { tag }, token: store.token }),

  // ---- org admin (DU-D10, Bearer = SESSION token; server gates on role) ----
  // Role probe: 200 = admin, 403 = member (same trick as org.html). Demo: member.
  orgUsage: (session: string) => req<OrgUsage>("/v1/org/usage", { token: session }),
  orgUsers: (session: string) => req<OrgUserSummary[]>("/v1/org/users", { token: session }),
  updateOrgUser: (session: string, id: string, body: { role?: string; disabled?: boolean; password?: string }) =>
    req<void>(`/v1/org/users/${encodeURIComponent(id)}`, { method: "PATCH", body, token: session }),
  createOrgProject: (session: string, name: string) =>
    req<void>("/v1/org/projects", { method: "POST", body: { name }, token: session }),
  orgProjectMembers: (session: string, project: string) =>
    req<ProjectMemberSummary[]>(`/v1/org/projects/${encodeURIComponent(project)}/members`, { token: session }),
  addOrgProjectMember: (session: string, project: string, userId: string) =>
    req<void>(`/v1/org/projects/${encodeURIComponent(project)}/members/${encodeURIComponent(userId)}`, { method: "POST", token: session }),
  removeOrgProjectMember: (session: string, project: string, userId: string) =>
    req<void>(`/v1/org/projects/${encodeURIComponent(project)}/members/${encodeURIComponent(userId)}`, { method: "DELETE", token: session }),
};

// Live delivery. peek=true so the desktop never steals the identity's real
// inbox (same rule the TUI follows). Reconnects by itself with growing waits
// (1s, 2s, 4s … capped at 30s, + jitter) and reports "connected"/"reconnecting"
// through onState — peek has no replay, so callers re-fetch history on every
// transition back to "connected". Returns a disconnect fn (stops reconnecting).
export type WsState = "connecting" | "connected" | "reconnecting";
export function connectWs(onDelivery: (d: Delivery) => void, onState?: (s: WsState) => void): () => void {
  if (store.demo) {
    onState?.("connected"); // no server in demo mode
    return () => {};
  }
  const wsUrl = store.server.replace(/^http/, "ws") + `/v1/ws?peek=true`;
  const token = store.token; // pinned: a project switch must not retarget this socket
  let stopped = false;
  let attempt = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let ws: WebSocket | undefined;

  const retry = () => {
    if (stopped || timer) return;
    onState?.("reconnecting");
    const wait = Math.min(30_000, 1000 * 2 ** attempt) + Math.random() * 400;
    attempt++;
    timer = setTimeout(() => {
      timer = undefined;
      open();
    }, wait);
  };
  const open = async () => {
    if (stopped) return;
    if (attempt === 0) onState?.("connecting");
    let sock: WebSocket;
    try {
      // a connect that never settles must not stall reconnection forever: give up after 10s
      const attemptP = WebSocket.connect(wsUrl, { headers: { Authorization: `Bearer ${token}` } });
      attemptP.then((late) => { if (timedOut) late.disconnect().catch(() => {}); }, () => {});
      let timedOut = false;
      sock = await Promise.race([
        attemptP,
        new Promise<never>((_, rej) => setTimeout(() => { timedOut = true; rej(new Error("connect timeout")); }, 10_000)),
      ]);
    } catch (e) {
      if (/\b401\b/.test(String(e)) && !stopped) authExpiredHandler?.();
      retry();
      return;
    }
    if (stopped) {
      sock.disconnect().catch(() => {});
      return;
    }
    ws = sock;
    attempt = 0;
    onState?.("connected");
    sock.addListener((msg) => {
      if (msg.type === "Text") {
        try {
          onDelivery(JSON.parse(msg.data as string));
        } catch {
          /* non-JSON control frame */
        }
      } else if (msg.type === "Close") {
        if (ws === sock) ws = undefined;
        retry();
      }
    });
  };
  open();
  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    ws?.disconnect().catch(() => {});
  };
}
