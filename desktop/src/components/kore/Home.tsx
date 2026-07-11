// Home — org landing, dashboard-style (Centaury /client language). Live mode
// (DU-D3): real projects from GET /v1/user/projects (name + instance_count) +
// org name from login. Stats/bar are REAL counts (no fabricated finance). The
// org-people strip needs GET /v1/org/members (DU-S4, not built) so it only
// renders in demo. Demo mode keeps the full mockOrg preview.
import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { ArrowUpRight, Cpu, FolderGit2, Users } from "lucide-react";
import { api, store, type ProjectSummary } from "@/lib/api";
import { mockOrg } from "@/lib/mock";
import { AvatarStack } from "@/components/kore/AvatarStack";

type Card = {
  name: string;
  featured: boolean;
  count: number;
  stat: string;
  people?: string[]; // demo only — no org-members endpoint yet (DU-S4)
  totalPeople?: number;
};

function ProjectCard({ p, i, onOpen }: { p: Card; i: number; onOpen: (name: string) => void }) {
  return (
    <motion.button
      initial={{ opacity: 0, y: 12 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ delay: i * 0.05 }}
      onClick={() => onOpen(p.name)}
      className={cnCard(p.featured)}
    >
      <div className="flex h-full flex-col p-5 text-left">
        <div className="flex items-start justify-between">
          <div className="grid h-9 w-9 place-items-center rounded-lg bg-muted text-muted-foreground">
            <FolderGit2 className="h-4 w-4" />
          </div>
          <ArrowUpRight className="h-4 w-4 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100" />
        </div>
        <div className="flex-1" />
        <h3 className={p.featured ? "k-title text-2xl" : "k-title text-base"}>{p.name}</h3>
        <div className="mt-1 flex items-center gap-3 text-xs font-medium text-muted-foreground">
          <span className="inline-flex items-center gap-1"><Cpu className="h-3.5 w-3.5" /> {p.stat}</span>
        </div>
        {p.people && (
          <div className="mt-3 flex items-center justify-between">
            <AvatarStack seeds={p.people} total={p.totalPeople ?? p.people.length} max={p.featured ? 6 : 3} size={p.featured ? 32 : 26} />
            {p.featured && <span className="text-[11px] text-muted-foreground">working here</span>}
          </div>
        )}
      </div>
    </motion.button>
  );
}

function cnCard(featured?: boolean) {
  return [
    "group relative overflow-hidden rounded-2xl border bg-card shadow-sm transition-all hover:-translate-y-0.5 hover:border-foreground/20 hover:shadow-md",
    featured ? "col-span-2 row-span-2 min-h-[300px]" : "min-h-[144px]",
  ].join(" ");
}

function StatTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col justify-center rounded-xl bg-card p-4">
      <span className="k-title text-2xl">{value}</span>
      <span className="text-xs text-muted-foreground">{label}</span>
    </div>
  );
}

// per-project bar of instance counts — real data, /client bar look (dark cap +
// slate-blue gradient body). Slate rgba = brand #777fa8, reads in both themes.
function ProjectBars({ cards }: { cards: Card[] }) {
  const max = Math.max(1, ...cards.map((c) => c.count));
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <h2 className="k-title text-base">Instances per project</h2>
      <div className="mt-5 flex h-[150px] items-end gap-2">
        {cards.map((c) => (
          <div key={c.name} className="flex min-w-0 flex-1 flex-col items-center justify-end gap-2">
            <div className="flex w-full flex-col justify-end" style={{ height: `${(c.count / max) * 100}%`, minHeight: 4 }}>
              <span className="h-[4px] rounded-t-sm bg-foreground/80" />
              <span
                className="flex-1 rounded-b-sm"
                style={{ background: "linear-gradient(180deg, rgba(119,127,168,0.5) 0%, rgba(119,127,168,0.05) 100%)" }}
              />
            </div>
            <span className="w-full truncate text-center text-[10px] text-muted-foreground">{c.name}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function Home({ onOpenProject }: { onOpenProject: (name: string) => void }) {
  const [live, setLive] = useState<ProjectSummary[] | null>(null);

  useEffect(() => {
    if (store.demo || !store.session) return;
    api.userProjects(store.session).then(setLive).catch(() => {});
  }, []);

  const demo = store.demo;
  const orgName = demo ? mockOrg.name : store.org ?? "your org";
  const cards: Card[] = demo
    ? mockOrg.projects.map((p) => ({
        name: p.name,
        featured: !!p.featured,
        count: p.agents,
        stat: `${p.agents} agents`,
        people: p.people,
        totalPeople: p.totalPeople,
      }))
    : (live ?? []).map((p, i) => ({
        name: p.name,
        featured: i === 0,
        count: p.instance_count,
        stat: `${p.instance_count} instances`,
      }));

  const totalInstances = cards.reduce((s, c) => s + c.count, 0);

  return (
    <div className="flex h-full flex-col gap-5 overflow-y-auto p-6">
      {/* top bar */}
      <header className="flex items-center justify-between">
        <div>
          <p className="text-xs font-medium uppercase tracking-widest text-muted-foreground">Organization</p>
          <h1 className="k-title text-3xl">{orgName}</h1>
        </div>
        {demo && (
          <span className="rounded-full border border-dashed px-3 py-1 font-mono text-[10px] uppercase text-muted-foreground">preview</span>
        )}
      </header>

      {/* stats / chart well — real Kore data, /client charts-panel language */}
      <div className="grid gap-5 rounded-2xl bg-muted p-5 md:grid-cols-[1.25fr_1fr]">
        <ProjectBars cards={cards} />
        <div className="grid grid-cols-2 gap-3">
          <StatTile label="Projects" value={String(cards.length)} />
          <StatTile label="Instances" value={String(totalInstances)} />
          <StatTile label="People" value={demo ? String(mockOrg.totalPeople) : "—"} />
          {demo && (
            <div className="col-span-1 flex items-center rounded-xl bg-card p-4">
              <AvatarStack seeds={mockOrg.people} total={mockOrg.totalPeople} max={5} size={30} />
            </div>
          )}
        </div>
      </div>

      {/* projects bento */}
      <section className="k-card p-5">
        <h2 className="mb-3 text-sm font-semibold text-muted-foreground">Projects</h2>
        {!demo && cards.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No projects yet — open one by joining it at login, or summon an agent into a new one.
          </p>
        )}
        <div className="grid auto-rows-[144px] grid-cols-2 gap-4 lg:grid-cols-4">
          {cards.map((p, i) => (
            <ProjectCard key={p.name} p={p} i={i} onOpen={onOpenProject} />
          ))}
        </div>
        {!demo && (
          <p className="mt-4 text-[11px] text-muted-foreground">
            <Users className="mr-1 inline h-3 w-3" />
            Org member list arrives with the org-members endpoint (DU-S4).
          </p>
        )}
      </section>
    </div>
  );
}
