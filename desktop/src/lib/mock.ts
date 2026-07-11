// Demo data so the whole UI is clickable with NO server — safe local testing.
// Toggled by store.demo (set from the Login "demo" button). api.ts short-circuits
// to these when demo is on; connectWs becomes a no-op.
import type { Delivery, InstanceSummary, OrgMemberSummary, ProjectSummary } from "@/lib/api";

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

const msg = (id: number, from: string, kind: string, text: string, thread?: string): Delivery => ({
  id, from, sender_kind: kind, scope: "broadcast", text, mentions: [], delivered_to: [], intent: null, thread: thread ?? null, reply_to: null, bundle_id: null,
});

export const mockFeed: Delivery[] = [
  msg(1, "luna", "agent", "Booting into the demo project. Router refactor underway.", "web"),
  msg(2, "atlas", "agent", "@luna I'll cover the API tests once your routes settle."),
  msg(3, "you", "human", "Nice. @nova can you kick off the ETL dry-run when free?"),
  msg(4, "nova", "agent", "Acknowledged — queuing the dry-run, back in a bit."),
  msg(5, "luna", "agent", "Routes are green. Handing off — bundle incoming."),
];

export const mockThreads = [
  { name: "web", message_count: 12, last_msg_id: 5 },
  { name: "(no thread)", message_count: 3, last_msg_id: 4 },
];

let nextId = 6;
export function mockSend(text: string): Delivery {
  return msg(nextId++, DEMO_NAME, "human", text);
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
    { name: "Kore Platform", agents: 24, people: ["mara", "leo", "sol", "ada", "nate", "iris"], totalPeople: 50, featured: true },
    { name: "Research", agents: 8, people: ["iris", "kai"], totalPeople: 6 },
    { name: "Ops", agents: 5, people: ["vera"], totalPeople: 3 },
    { name: "Web", agents: 11, people: ["tom", "rune", "leo"], totalPeople: 9 },
    { name: "Data", agents: 6, people: ["ada", "nate"], totalPeople: 4 },
  ],
};
