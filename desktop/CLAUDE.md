# kore-desktop — map

GUI kore-client (Tauri v2 + React 19 + Vite + Tailwind v4 + shadcn). Thin:
connections + UI only, **zero product logic** (SaaS split — logic → server/).
Full context + endpoint contract + UI-lib policy + screen plan:
`docs/desktop/DESKTOP-APP.md` (read it before touching this crate).

Replaced the frozen pre-SaaS concept-ware, now archived at `desktop-legacy/`
(out of workspace — never read/port it). DECISIONS "Desktop app unfrozen".

## File map

| File | Owns |
|---|---|
| `src-tauri/src/lib.rs` | Tauri builder; registers plugins: opener + **http** (frontend fetch through Rust → no CORS) + **websocket** (Bearer header auth for `/v1/ws`). Custom commands (DU-D1 lite): `detect_tools` (PATH probe of the 12 tool binaries, cursor→cursor-agent) + `spawn_launch` (shells to `kore-client launch` — writes the app's instance token to `$KORE_DIR/<project>/token`, the same file `kore-client human` writes, so CLI and app share identity; `kore_bin()` resolves the bundled sidecar next to the app exe first, PATH fallback for dev shells — C1) + `kill_agent` (DU-D6: shells to `kore-client kill <name>` — server delete + tombstone + local pid SIGTERM; no --go, app shell has no KORE_NAME) |
| `src-tauri/src/main.rs` | `kore_desktop_lib::run()` |
| `src-tauri/Cargo.toml` | crate `kore-desktop`, lib `kore_desktop_lib`; tauri + http/websocket/opener plugins; updater + process under a desktop-only target table (C5) |
| `src-tauri/capabilities/default.json` | permissions: core (+ `core:window/webview/app:default` + `webview:allow-create-webview-window` for custom titlebar controls & About child window), opener, `http:default` (localhost + https scope), `websocket:default`, `updater:default` + `process:allow-restart` (C5 About flow). Windows: `main`, `about` |
| `src-tauri/tauri.conf.json` | productName "Kore", identifier com.centaury.kore, 1100×740; main window `decorations:false` (custom titlebar); `bundle.externalBin` ships kore-client as sidecar (C1) — `scripts/sidecar.sh` (chained into beforeBuildCommand) builds + stages it at `src-tauri/binaries/kore-client-<host-triple>` (gitignored); C5: `plugins.updater` (pubkey + endpoint = mirror raw `desktop/latest.json` — raw-file on main, NOT `releases/latest/download` which breaks when a CLI `v*` release lands on top) + `bundle.createUpdaterArtifacts` (needs `TAURI_SIGNING_PRIVATE_KEY` env at build — value may be key content **or a path**; there is NO `_PATH` variant in Tauri v2). `tauri.smoke.json` = `--config` overlay → localhost endpoint for local smoke |
| `src/lib/api.ts` | Kore client: wire types (mirror kore-protocol) + `req()` over http plugin + `connectWs()` over ws plugin (peek=true) + `store` (localStorage session/token/project/name/**demo**). Every read/send short-circuits to `lib/mock.ts` when `store.demo`. Writes: `send`, `setStatus` (PATCH /v1/instances/self), `retag` (PATCH /v1/instances/{name}, owner-or-self) |
| `src/lib/mock.ts` | Demo data (roster/feed/projects/threads) + `mockOrg` (Home org preview: humans + bento projects) for server-free testing |
| `src/lib/theme.tsx` | next-themes provider + ThemeToggle |
| `src/lib/utils.ts` | `cn()` |
| `src/components/ui/` | shadcn primitives + `gradient-heading` (@cult-ui) |
| `src/components/kokonutui/` | `beams-background` (unwired since login bg swap), `ai-prompt` (unwired) |
| `src/components/smoothui/agent-avatar/` | generative per-seed avatar (Home/AvatarStack + roster + messages) |
| `src/components/kore/sprites.tsx` | hand-authored pixelart: `TOOLS` (10 dialects, brand palette) + `PixelSprite` (inline SVG robot per tool). Wire.tsx's art |
| `src/components/kore/AvatarStack.tsx` | overlapping circular avatars + `+N` overflow (reuses AgentAvatar) |
| `src/components/kore/GithubSquares.tsx` | login background: GitHub contribution-graph grid (green squares, twinkle, radial mask) |
| `src/components/kore/Login.tsx` | login → pick project → register-human (session→instance) + demo-mode button. NO name field — server assigns it from the account's owner_name (DU-S2), read back from `RegisterResponse.name`. Bg = GithubSquares (fixed inset, no scroll) |
| `src/components/kore/Home.tsx` | **landing** — projects bento (featured double-size), click a card → `onOpenProject`. LIVE mode (DU-D3): real projects from `/v1/user/projects` + org name from login (`store.org`); people strip demo-only until DU-S4's org-members endpoint. Demo keeps the full `mockOrg` preview |
| `src/components/kore/ProjectWorkspace.tsx` | the bubble you enter on project click: back header (+ **Summon** button → SummonForm) + **Chats** (main) + roster/broadcast side panel (roster grouped by owner + broadcast composer). Broadcast POSTs `/v1/messages` no-targets (mock in demo); Chats owns its own feed/peek WS (no local feed here anymore). Roster rows clickable → AgentDetail. DU-D8: 3s roster poll while open (status_context is the liveness truth; presence dot tooltips say connected≠ready, inactive≠dead). **The collaboration graph moved out to `Network.tsx`** — no canvas/toggle here (owner 2026-07-08) |
| `src/components/kore/SummonForm.tsx` | launch dialog (DU-D4): tool select from `detect_tools`; project = SELECT over `/v1/user/projects` with type-to-filter, NO free text (typos can't invent projects; creation is DU-S1's admin flow); owner = "you" default + searchable picker over `/v1/org/members` (DU-S4) to DELEGATE to another account (Q1, server-validated); count/tag/directory/extra-args; background toggle (default ON → `--headless --stay`, persistent per DU-C1; off → `--terminal` auto). Submits via `spawn_launch`; kore-client errors surface verbatim in a toast. Demo mode = info toast, no spawn |
| `src/components/kore/AgentDetail.tsx` | roster-row dialog (CLI `list <name>` parity): shows InstanceSummary fields + owner-or-self **retag** + self **set-status** + directed **send** (targets=[name]) + **kill** (DU-D6, agents only: native confirm() then Tauri `kill_agent`; demo = toast). All writes via api.ts, server enforces authz |
| `src/components/kore/CollabCanvas.tsx` | old SVG collaboration viz (hub-spoke, motion orbs) — **UNWIRED**, superseded by `Network.tsx` (React Flow). File kept |
| `src/components/kore/Network.tsx` | collaboration graph, its own page (moved out of ProjectWorkspace, owner 2026-07-08). React Flow (`@xyflow/react`): pan/zoom/drag/minimap/select. Two levels: **Org** = projects orbiting the Kore core (real `/v1/user/projects` counts) → click a project to drill; **Project** = its roster graph (humans ring + agents orbit + core). Positions computed here (radial), React Flow layers interaction. Live drill only for the JOINED project (`/v1/instances`); other projects show a "needs org-wide roster endpoint (DU-S)" note; demo shows the full mock bubble. Live-traffic edge pulse via peek WS (joined project) |
| `src/components/kore/Shell.tsx` | ground + floating white panel (Centaury /client look). Sidebar: brand · user card · nav tiles active=dark (Home/**Network**/Org/Settings) · Kore-ASCII card + DEMO + Sign-out. Project click → ProjectWorkspace overlays the main pane |
| `src/components/kore/Chats.tsx` | DU-D5 v1, WhatsApp-style (owner 2026-07-07 reversed the 2026-07-06 "no chat"): left = chat list (Global + per-instance rows, live status_context line); right = conversation. Global (R13/R17) = full feed, history + peek WS live, non-broadcasts render "from → mentions" (+· DM marker for agent↔owner pairs, R18); DM (R12) = feed filtered {me ↔ x}, composer targets=[x]. R14 pending DU-S3 (composer hints @ still narrows). Roster passed in from ProjectWorkspace (shares the 3s poll) |
| `src/components/kore/Chat.tsx` | old single-screen chat — **UNWIRED**, superseded by Chats.tsx. File kept |
| `src/components/kore/Roster.tsx` | presence cards — **UNWIRED** (folds into Home project drill-down later). File kept |
| `src/components/kore/Threads.tsx` | thread list — **UNWIRED** (threads become per-project admin-only broadcast later). File kept |
| `src/components/kore/OrgAdmin.tsx` | DU-D10 (R1/R3): org screen behind the sidebar "Org" item. Role probe = `GET /v1/org/usage` (200 admin / 403 member, org.html trick). Member (+demo): read-only directory (DU-S4 orgMembers). Admin: usage cards (R3, read-only — quota is platform-admin), accounts table (role toggle / disable / password reset via `PATCH /v1/org/users/{id}`), projects panel (create via DU-S1 `POST /v1/org/projects`; expand → project members with add/remove chips). Project list = `usage.projects` (no separate admin list endpoint). All writes SESSION token, server-gated; thin remote control |
| `src/components/kore/Settings.tsx` | server URL, theme, sign-out |
| `src/components/kore/TitleBar.tsx` | generic frameless titlebar: drag region + min/max/close controls (ported from desktop-legacy). `rightActions` slot |
| `src/components/kore/MainTitleBar.tsx` | main-window titlebar: theme toggle (next-themes) + About/updates button → opens `about` child window (`WebviewWindow`, url `/?window=about`) |
| `src/pages/About.tsx` | About child-window content: app version (`getVersion`) + real update flow (C5): `check()` → `downloadAndInstall()` with progress bar → `relaunch()`; phase machine idle/checking/latest/available/installing/ready/error. Own frame + TitleBar (no min/max). `scripts/latest-json.sh <version> <url-base>` emits the endpoint manifest from built sigs (release job reuses it) |
| `src/main.tsx` | mounts `About` when `?window=about`, else `App` (child-window routing, no router lib) |
| `src/App.tsx` | frame: `MainTitleBar` + content (`store.token` ? Shell : Login) |
| `src/index.css` | Tailwind v4 entry + **Centaury brand tokens** (white / `#2c2e3a` / slate-blue `#777fa8`, ported from web/globals.css; `--ground` var; dark = consistent slate) + `.k-*` component kit (card/well/pill-solid/pill-ghost/iconbtn/title = /client visual language) |

## Invariants

- Thin: no logic here; if a feature "works" client-side it's in the wrong crate.
- WS always `peek=true` — never consume the identity's inbox.
- HTTP via `tauri-plugin-http`, WS via `tauri-plugin-websocket` (CORS + header
  auth — don't swap to raw fetch/WebSocket).
- Wire types in `api.ts` mirror `kore-protocol`; protocol changes land there
  first — update `api.ts` when a DTO the app uses changes.
- New UI = pull from the 7 libraries first (DESKTOP-APP.md policy).

## Build

```
pnpm install && pnpm build      # frontend (tsc+vite)
pnpm tauri dev                  # full app (needs a running kore-server)
cargo check -p kore-desktop     # rust side
NO_STRIP=1 pnpm tauri build     # release bundles (deb/rpm/AppImage, C2);
                                # + TAURI_SIGNING_PRIVATE_KEY=~/.tauri/kore-updater.key
                                #   TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
                                #   to sign updater artifacts (C5; key NOT in repo —
                                #   back it up: losing it bricks the update chain)
                                # NO_STRIP: linuxdeploy's old strip chokes on
                                # .relr.dyn (modern binutils) — harmless elsewhere
```
esbuild native build is allow-listed in `pnpm-workspace.yaml` (`allowBuilds`).
