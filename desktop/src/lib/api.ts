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

// ---- session state (localStorage; token files are the CLI's job) ----
const K = {
  server: "kore.server",
  session: "kore.session_token",
  token: "kore.instance_token",
  project: "kore.project",
  name: "kore.name",
  org: "kore.org",
  demo: "kore.demo",
} as const;

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
  clear() {
    [K.session, K.token, K.project, K.name, K.org, K.demo].forEach((k) => localStorage.removeItem(k));
  },
};

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
    throw new Error(`${res.status} ${detail || res.statusText}`);
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
  instances: () =>
    store.demo ? Promise.resolve(mock.mockRoster) : req<InstanceSummary[]>("/v1/instances", { token: store.token }),
  history: (limit = 50, thread?: string) =>
    store.demo
      ? Promise.resolve(mock.mockFeed)
      : req<Delivery[]>(`/v1/messages?limit=${limit}${thread ? `&thread=${encodeURIComponent(thread)}` : ""}`, {
          token: store.token,
        }),
  unread: () => (store.demo ? Promise.resolve({ count: 0 }) : req<{ count: number }>("/v1/messages/unread", { token: store.token })),
  threads: () =>
    store.demo ? Promise.resolve(mock.mockThreads) : req<{ name: string; message_count: number; last_msg_id: number }[]>("/v1/threads", { token: store.token }),

  // send — in demo, no server; the caller echoes the local Delivery itself.
  send: (text: string, targets: string[] = [], reply_to?: number, thread?: string) =>
    store.demo
      ? Promise.resolve({ id: mock.mockSend(text).id })
      : req<{ id: number }>("/v1/messages", {
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
// inbox (same rule the TUI follows). Returns a disconnect fn.
export async function connectWs(onDelivery: (d: Delivery) => void, onClose?: () => void): Promise<() => void> {
  if (store.demo) return () => {}; // no server in demo mode
  const wsUrl = store.server.replace(/^http/, "ws") + `/v1/ws?peek=true`;
  const ws = await WebSocket.connect(wsUrl, { headers: { Authorization: `Bearer ${store.token}` } });
  ws.addListener((msg) => {
    if (msg.type === "Text") {
      try {
        onDelivery(JSON.parse(msg.data as string));
      } catch {
        /* non-JSON control frame */
      }
    } else if (msg.type === "Close") {
      onClose?.();
    }
  });
  return () => ws.disconnect();
}
