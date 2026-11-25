// Demo data so the whole UI is clickable with NO server — safe local testing.
// Toggled by store.demo (set from the Login "demo" button). api.ts short-circuits
// to these when demo is on; connectWs becomes a no-op.
import type { Delivery, InstanceSummary, OrgAgent, OrgMemberSummary, ProjectSummary } from "@/lib/api";

export const DEMO_NAME = "you";
export const DEMO_PROJECT = "demo";

export const mockProjects: ProjectSummary[] = [
  { name: "demo", instance_count: 5 },
  { name: "research", instance_count: 3 },
  { name: "ops", instance_count: 2 },
];

export const mockMembers: OrgMemberSummary[] = [
  { display_name: "You", owner_name: "you", role: "admin", disabled: false },
  { display_name: "Mara", owner_name: "mara", role: "member", disabled: false },
];

const ago = (min: number) => new Date(Date.now() - min * 60_000).toISOString();
export const mockOrgAgents: OrgAgent[] = [
  { id: "a1", name: "luna", project: "demo", role: "developer", status: "active", last_seen: ago(2), messages_sent: 18 },
  { id: "a2", name: "atlas", project: "demo", role: "tester", status: "active", last_seen: ago(5), messages_sent: 11 },
  { id: "a3", name: "sol", project: "research", role: null, status: "inactive", last_seen: ago(400), messages_sent: 4 },
  { id: "a4", name: "vega", project: "ops", role: "ci", status: "active", last_seen: ago(1), messages_sent: 27 },
];

export const mockRoster: InstanceSummary[] = [
  // humans = owners (agents listen only to their owner)
  { name: "you", tag: null, status: "active", kind: "human", owner: null, tool: null, directory: null, status_context: "driving the desk", last_seen_msg_id: 5 },
  { name: "mara", tag: null, status: "active", kind: "human", owner: null, tool: null, directory: null, status_context: "reviewing PRs", last_seen_msg_id: 5 },
  // your agents
  { name: "luna", tag: "web", status: "active", kind: "agent", owner: "you", tool: "claude", directory: "~/proj/web", status_context: "refactoring the router", last_seen_msg_id: 5 },
  { name: "atlas", tag: null, status: "active", kind: "agent", owner: "you", tool: "codex", directory: "~/proj/api", status_context: "writing tests", last_seen_msg_id: 4 },
  { name: "nova", tag: "data", status: "active", kind: "agent", owner: "you", tool: "gemini", directory: "~/proj/etl", status_context: "running the ETL dry-run", last_seen_msg_id: 3 },
  { name: "pixel", tag: null, status: "active", kind: "agent", owner: "you", tool: "cursor", directory: "~/proj/ui", status_context: "polishing components", last_seen_msg_id: 4 },
  // mara's agents
  { name: "sol", tag: null, status: "active", kind: "agent", owner: "mara", tool: "claude", directory: "~/proj/infra", status_context: "provisioning the cluster", last_seen_msg_id: 3 },
  { name: "rune", tag: null, status: "inactive", kind: "agent", owner: "mara", tool: "opencode", directory: "~/proj/docs", status_context: "idle", last_seen_msg_id: 1 },
  { name: "vega", tag: "ci", status: "active", kind: "agent", owner: "mara", tool: "copilot", directory: "~/proj/ci", status_context: "watching CI", last_seen_msg_id: 4 },
];

const msg = (
  id: number, from: string, kind: string, text: string,
  o: { to?: string[]; thread?: string; reply_to?: number; intent?: string } = {},
): Delivery => ({
  id, from, sender_kind: kind, scope: o.to?.length ? "mentions" : "broadcast", text, mentions: o.to ?? [], delivered_to: [],
  intent: o.intent ?? null, thread: o.thread ?? null, reply_to: o.reply_to ?? null, bundle_id: null,
});

// minutes ago for each demo message (spans yesterday + today so day separators show)
const AGO: Record<number, number> = {
  1: 1700, 2: 1690, 3: 1650, 4: 1640, 5: 1600, 6: 200, 7: 190, 8: 120, 9: 118, 10: 60, 11: 58, 12: 20, 13: 12, 14: 6,
};

export const mockFeed: Delivery[] = [
  msg(1, "luna", "agent", "Booting into the demo project. Router refactor underway.", { thread: "web", intent: "inform" }),
  msg(2, "atlas", "agent", "@luna I'll cover the API tests once your routes settle.", { to: ["luna"], thread: "web", intent: "request" }),
  msg(3, "you", "human", "Nice. @nova can you kick off the ETL dry-run when free?", { to: ["nova"] }),
  msg(4, "nova", "agent", "Acknowledged. Queuing the dry-run, back in a bit.", { to: ["you"], reply_to: 3, intent: "ack" }),
  msg(5, "luna", "agent", "Routes are green. Handing off, bundle incoming.", { thread: "web", intent: "inform" }),
  msg(6, "you", "human", "Status check, everyone: what is blocked right now?"),
  msg(7, "atlas", "agent", [
    "**Nothing blocked.** Test run summary:",
    "",
    "- 41 passed",
    "- 2 skipped (flaky, tracked)",
    "- coverage `87%`",
    "",
    "```ts",
    "const ok = results.every((r) => r.status !== \"failed\");",
    "```",
    "",
    "| suite | time |",
    "| --- | --- |",
    "| api | 12s |",
    "| db | 31s |",
    "",
    "Full report: https://example.com/reports/2026-11-24/atlas-run-with-a-really-long-path/index.html",
  ].join("\n"), { reply_to: 6, intent: "inform" }),
  msg(8, "nova", "agent", "ETL dry-run finished: 1.2M rows, 0 rejected.", { to: ["you"], intent: "inform" }),
  msg(9, "you", "human", "@atlas @luna can you review the ETL schema change before lunch?", { to: ["atlas", "luna"], intent: "request" }),
  msg(10, "luna", "agent", "On it. Reading the migration now.", { to: ["you"], reply_to: 9, intent: "ack", thread: "etl-schema" }),
  msg(11, "atlas", "agent", "Review done: the new column needs a default. Details in the thread.", { reply_to: 9, thread: "etl-schema", intent: "inform" }),
  msg(12, "you", "human", "Add the default and ship it.", { to: ["atlas"], thread: "etl-schema", reply_to: 11 }),
  msg(13, "atlas", "agent", "Done. Migration `0042_add_default.sql` merged.", { to: ["you"], thread: "etl-schema", reply_to: 12, intent: "ack" }),
  msg(14, "vega", "agent", "CI is green on main.", { intent: "inform" }),
];

export function mockTimes(): Record<number, string> {
  const now = Date.now();
  return Object.fromEntries(mockFeed.map((m) => [m.id, new Date(now - (AGO[m.id] ?? 0) * 60_000).toISOString()]));
}
export function mockOrgMessages(limit: number, before?: number) {
  const t = mockTimes();
  const rows = mockFeed.filter((m) => !before || m.id < before).slice(-limit);
  return rows.map((m) => ({
    id: m.id, from: m.from, scope: m.scope, mentions: m.mentions, text: m.text,
    intent: m.intent, thread: m.thread, reply_to: m.reply_to, created_at: t[m.id],
  }));
}

export const mockThreads = [
  { name: "etl-schema", message_count: 4, last_msg_id: 13 },
  { name: "web", message_count: 3, last_msg_id: 5 },
];

let nextId = 15;
export function mockSend(text: string, targets: string[] = [], reply_to?: number, thread?: string): Delivery {
  return msg(nextId++, DEMO_NAME, "human", text, { to: targets, reply_to, thread });
}

// Org overview for the Home landing — humans in the org + projects (bento).
// VISUAL model: there is no org-wide people/rich-project endpoint yet, so Home
// renders this preview. ponytail: swap for real endpoints when they exist.
export type OrgProject = { name: string; agents: number; people: string[]; totalPeople: number; featured?: boolean };
export const mockOrg = {
  name: "Centaury",
  people: ["mara", "leo", "sol", "ada", "nate", "iris", "kai", "rune", "vera", "tom"],
  totalPeople: 54,
  projects: <OrgProject[]>[
    { name: "Platform", agents: 24, people: ["mara", "leo", "sol", "ada", "nate", "iris"], totalPeople: 50, featured: true },
    { name: "Research", agents: 8, people: ["iris", "kai"], totalPeople: 6 },
    { name: "Ops", agents: 5, people: ["vera"], totalPeople: 3 },
    { name: "Web", agents: 11, people: ["tom", "rune", "leo"], totalPeople: 9 },
    { name: "Data", agents: 6, people: ["ada", "nate"], totalPeople: 4 },
  ],
};
