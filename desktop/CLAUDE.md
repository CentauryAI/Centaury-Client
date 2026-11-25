# Centaury desktop — map

GUI centaury (Tauri v2 + React 19 + Vite + Tailwind v4 + shadcn). Thin:
connections + UI only, **zero product logic** (SaaS split — logic → server/).
Full context + endpoint contract + UI-lib policy + screen plan:
`docs/desktop/DESKTOP-APP.md` (read it before touching this crate).
What is done, what is still open and what needs a server change first:
`STATUS.md`.

Replaced the frozen pre-SaaS concept-ware, now archived at `desktop-legacy/`
(out of workspace — never read/port it). DECISIONS "Desktop app unfrozen".

## File map

| File | Owns |
|---|---|
| `src-tauri/src/lib.rs` | Tauri builder; registers plugins: opener + **http** (frontend fetch through Rust → no CORS) + **websocket** (Bearer header auth for `/v1/ws`). Custom commands (DU-D1 lite): `detect_tools` (PATH probe of the 12 tool binaries, cursor→cursor-agent) + `spawn_launch` (shells to `centaury launch` — writes the app's instance token to `$KORE_DIR/<project>/token`, the same file `centaury human` writes, so CLI and app share identity; `kore_bin()` resolves the bundled sidecar next to the app exe first, PATH fallback for dev shells — C1) + `kill_agent` (DU-D6: shells to `centaury kill <name>` — server delete + tombstone + local pid SIGTERM; no --go, app shell has no KORE_NAME) |
| `src-tauri/src/main.rs` | `centaury_desktop_lib::run()` |
| `src-tauri/Cargo.toml` | crate `centaury-desktop`, lib `centaury_desktop_lib`; tauri + http/websocket/opener plugins; updater + process under a desktop-only target table (C5) |
| `src-tauri/capabilities/default.json` | permissions: core (+ `core:window/webview/app:default` + `webview:allow-create-webview-window` for custom titlebar controls & About child window), opener, `http:default` (localhost + https scope), `websocket:default`, `updater:default` + `process:allow-restart` (C5 About flow). Windows: `main`, `about` |
| `src-tauri/tauri.conf.json` | productName "Centaury", window title "Centaury", identifier com.centaury.client, 1100×740; main window `decorations:false` (custom titlebar); `bundle.externalBin` ships centaury as sidecar (C1) — `scripts/sidecar.sh` (chained into beforeBuildCommand) builds + stages it at `src-tauri/binaries/centaury-<target-triple>[.exe]` (target = `TAURI_ENV_TARGET_TRIPLE` from the tauri CLI, else host; gitignored); C5: `plugins.updater` (pubkey + endpoint = mirror raw `desktop/latest.json` — raw-file on main, NOT `releases/latest/download` which breaks when a CLI `v*` release lands on top) + `bundle.createUpdaterArtifacts` (needs `TAURI_SIGNING_PRIVATE_KEY` env at build — value may be key content **or a path**; there is NO `_PATH` variant in Tauri v2). `tauri.smoke.json` = `--config` overlay → localhost endpoint for local smoke. `lib.rs` setup runs a log-only startup update probe (`[updater] …` on stderr; 404/offline/bad JSON swallowed) |
| `src/lib/api.ts` | Kore client: wire types (mirror kore-protocol) + `req()` over http plugin + `connectWs()` over ws plugin (peek=true) + `store` (localStorage session/token/project/name/**demo**). Every read/send short-circuits to `lib/mock.ts` when `store.demo`. Writes: `send`, `setStatus` (PATCH /v1/instances/self), `retag` (PATCH /v1/instances/{name}, owner-or-self) |
| `src/lib/useChatFeed.ts` | the ONE feed + send path of a project (the single composer in Chats uses it): history load states, WS with auto-reconnect + re-sync on reconnect/focus, optimistic outgoing bubbles (sending → sent with POST `id` / failed + manual retry, never auto-resend), dedupe by `id`. `api.ts` also holds `ApiError`/`errorInfo` (401/403/429/5xx/network), per-project instance tokens (`store.tokenFor/activate`) and `onAuthExpired` (any 401 → login). `kore/QueryState.tsx` = loading / empty / error+retry view |
| `src/lib/mock.ts` | Demo data (roster/feed/projects/threads) + `mockOrgAgents` + `mockOrg` (Network demo bubble) for server-free testing |
| `src/lib/time.ts` | `ago()` / `exact()` for org screens (the server mixes RFC 3339 and Postgres-style stamps) |
| `src/lib/theme.tsx` | next-themes provider (light default, `system` allowed) + ThemeToggle |
| `src/lib/prefs.ts`, `src/lib/unread.ts` | local preferences (`kore.pref.*`, `usePref`) and the global unread total (external store: Chats writes, Shell reads) |
| `src/lib/utils.ts` | `cn()` |
| `src/components/centaury/` | vendored copy of the `@centaury/ui` design system (`shell`, `auth`, `charts`, `primitives`, `lib`) — the house components every screen builds on. A copy, not a dependency (the package uses `next/link`/`next/font`); differences are listed in its `README.md` |
| `src/styles/centaury/` | design-system CSS: `theme.css` (tokens + `dark` variant), `brands.css` (`--primary`/`--accent` brand ramp), `base.css`, `motion.css` |
| `src/assets/fonts/`, `src/assets/centaury/` | house fonts; `logo.svg` + the `hands.webp`/`field.webp` textures |
| `src/components/ui/` | shadcn primitives + `gradient-heading` (@cult-ui). Hover/focus surfaces use `muted` — `--accent` is the brand color, never a hover |
| `src/components/kokonutui/` | `beams-background` (unwired since login bg swap), `ai-prompt` (unwired) |
| `src/components/smoothui/agent-avatar/` | generative per-seed avatar (Home/AvatarStack + roster + messages) |
| `src/components/kore/sprites.tsx` | hand-authored pixelart: `TOOLS` (10 dialects, slate ramp — no per-tool hues) + `PixelSprite` (inline SVG robot per tool; inverted under `.dark` so it stays visible) |
| `src/components/kore/AvatarStack.tsx` | overlapping circular avatars + `+N` overflow (reuses AgentAvatar) |
| `src/components/kore/Login.tsx` | login → pick project → register-human (session→instance) + demo-mode button. NO name field — server assigns it from the account's owner_name (DU-S2), read back from `RegisterResponse.name`. Layout mirrors centaury.ai `/login`: `AuthShell` card with the form on the left and `QuotePanel` (static `hands.webp` texture, no shader/three.js — WebKitGTK) on the right; fits 1100×740 without scrolling |
| `src/components/kore/Home.tsx` | **landing** — answers "where do I continue?": «Continue in <active project>» card (its agents from `/v1/org/agents`, **Open project** → `onOpenProject`), the other projects as plain cards, and one row of real figures only (projects, members from `/v1/org/members`, agents/max_agents from `/v1/user/me`; a figure whose request fails is omitted). No org name (the sidebar has it), no chart. Demo runs the same path on `lib/mock.ts` |
| `src/components/kore/ProjectWorkspace.tsx` | the bubble you enter on project click: back header (+ **Summon** button → SummonForm) + **Chats** (main) + roster side panel (grouped by owner; no composer there — the only one is in Chats). Owns the project's `useChatFeed()` and hands it to Chats. Header shows the WS state (Connected / Reconnecting…). Mounted by Shell with `key={project}` so a project switch resets feed, roster and socket. Roster rows clickable → AgentDetail. DU-D8: 3s roster poll while open (status_context is the liveness truth; presence dot tooltips say connected≠ready, inactive≠dead). **The collaboration graph moved out to `Network.tsx`** — no canvas/toggle here (owner 2026-07-08) |
| `src/components/kore/SummonForm.tsx` | launch dialog (DU-D4): basics up top (tool from `detect_tools`, count, project = SELECT over `/v1/user/projects` with type-to-filter, no free text); **Advanced** (folded): who controls it (owner picker over `/v1/org/members`, Q1 delegation), tag, start folder, extra options for the tool, background toggle (default ON → `--headless --stay`; off → `--terminal`). One-line summary of what will start before the button. Submits via `spawn_launch`; centaury errors verbatim in a toast. Demo = info toast |
| `src/components/kore/AgentDetail.tsx` | roster-row dialog: state in words (Connected / Not listening + status_context) → **Message** (targets=[name]) → details → your status (self) / **Tag** with Save + Remove tag (owner-or-self) → "More" menu (agents only) → **Kill agent…** opens a confirmation `Dialog` naming the agent, then Tauri `kill_agent` (DU-D6; demo = toast). All writes via api.ts, server enforces authz |
| `src/components/kore/CollabCanvas.tsx` | old SVG collaboration viz (hub-spoke, motion orbs) — **UNWIRED**, superseded by `Network.tsx` (React Flow). File kept |
| `src/components/kore/Network.tsx` | collaboration graph, its own page. React Flow (`@xyflow/react`): pan/zoom/drag/minimap/select. **Graph \| List** switch (the list is a keyboard-accessible table of the same rows) + a short legend. Two levels: **Org** = projects orbiting the Centaury core (`/v1/user/projects`) → click to drill; **Project** = roster graph (humans ring + agents orbit + core). Roster = that project's own token (`api.instances(token)`: owners, tools) or, if never opened, the org directory `GET /v1/org/agents` (agents only, hung off the core). **Open project** button → `onOpenProject` (Shell passes `openProject`). No live-traffic pulses (the server gives a human no reliable stream). Errors via `QueryState`. Demo shows the full mock bubble |
| `src/components/kore/Shell.tsx` | ground + floating white panel (Centaury /client look). Sidebar: brand · user card · nav tiles active=dark (Home/**Network**/Org/Settings) · organisation card + Sign-out, built on the house `Sidebar`. Unread: the sidebar Home item carries a count badge (`lib/unread.ts`, fed by Chats) while a project's workspace is mounted but hidden — `go()` only hides it (`inWorkspace`), the feed/WS keep running; `visible` tells Chats it is not on screen (nothing gets marked read). Project click → `openProject()`: `registerHuman` if that project has no stored token, `store.activate`, then ProjectWorkspace overlays the main pane; a 403 stays on Home with a banner |
| `src/components/kore/Chats.tsx` | DU-D5 v1, WhatsApp-style (owner 2026-07-07 reversed the 2026-07-06 "no chat"): left = chat list (Global + per-instance rows with last message, unread badge, ordered by activity); right = conversation with filters All / Announcements / Directed / Threads, day separators and hours, "N new messages" button, per-conversation scroll memory. One composer (`Composer.tsx`): the line under it names who receives the message, `@` autocompletes. Bodies render through `Markdown.tsx` (no raw HTML, links open outside). Read state is local (`kore.read.*`), marked only with the conversation open, the window focused and `visible`. Global (R13/R17) = full feed, history + peek WS live, non-broadcasts render "from → mentions" (+· DM marker for agent↔owner pairs, R18); DM (R12) = feed filtered {me ↔ x}, composer targets=[x]. R14 pending DU-S3 (composer hints @ still narrows). Roster passed in from ProjectWorkspace (shares the 3s poll) |
| `src/components/kore/Chat.tsx` | old single-screen chat — **UNWIRED**, superseded by Chats.tsx. File kept |
| `src/components/kore/Roster.tsx` | presence cards — **UNWIRED** (folds into Home project drill-down later). File kept |
| `src/components/kore/Threads.tsx` | thread list — **UNWIRED** (threads become per-project admin-only broadcast later). File kept |
| `src/components/kore/OrgAdmin.tsx` | DU-D10 (R1/R3) + stage 4: org screen behind the sidebar "Org" item. Role comes from `GET /v1/user/me` (`role`). Member (+demo): Members directory + read-only **Agents** tab (`GET /v1/org/agents`, server-scoped to their projects). Admin: usage cards (read-only — quota is platform-admin) and tabs **Accounts** (role toggle / disable / password reset via `PATCH /v1/org/users/{id}`; each one opens a confirmation `Dialog` naming the account, reset needs ≥10 chars and shows its result, a 409 "last admin" message is shown verbatim), **Projects** (create via DU-S1 `POST /v1/org/projects`; expand → members add/remove chips; list = `usage.projects`), **Agents** (whole org) and **Audit** (`GET /v1/org/audit`, `before`/`limit`, Load more). All writes SESSION token, server-gated; thin remote control |
| `src/components/kore/Settings.tsx` | six sections behind pill tabs: **Account** (read-only `GET /v1/user/me`: name/email/role/org/plan + sign-out; the server has no profile-edit or password route), **Appearance** (light/dark/system + Reduce motion), **Notifications** (one switch: in-app notice for a message in another conversation), **Shortcuts** (read-only list of the keys that really exist), **Connection** (server URL), **About** (version + Check for updates → opens the About window, no second updater). Preferences = `lib/prefs.ts` (`kore.pref.notify` default on, `kore.pref.reduceMotion` default off → `<html data-reduce-motion>`, CSS in `index.css`) |
| `src/components/kore/TitleBar.tsx` | generic frameless titlebar: drag region + min/max/close controls (ported from desktop-legacy). `rightActions` slot. Window controls are guarded by `inTauri`, so the UI also renders in a plain browser (`pnpm dev` on :1420) for visual checks |
| `src/components/kore/MainTitleBar.tsx` | main-window titlebar: theme toggle (next-themes) + About/updates button → opens `about` child window (`WebviewWindow`, url `/?window=about`) |
| `src/pages/About.tsx` | About child-window content: app version (`getVersion`) + real update flow (C5): `check({timeout})` → `downloadAndInstall()` with progress bar → `relaunch()`; phase machine idle/checking/latest/available/installing/ready/error; check/install failures show "Couldn't check for updates…" + detail and re-enable the button. Own frame + TitleBar (no min/max). `node scripts/latest-json.mjs --base-url <url> [--version] [--notes] [--out] [dir…]` emits the Tauri v2 static manifest from built `.sig`s (release.yml `desktop-release` job uses it) |
| `src/main.tsx` | mounts `About` when `?window=about`, else `App` (child-window routing, no router lib) |
| `src/App.tsx` | frame: `MainTitleBar` + content (`store.token` ? Shell : Login) |
| `src/index.css` | Tailwind v4 entry: imports `styles/centaury/*`, declares the `@font-face`s, the `.dark` token overrides (light is the default — the web has no dark theme; dark is kept through tokens such as `bg-card`), the `.k-*` component kit and the title-bar rules |

## Invariants

- Thin: no logic here; if a feature "works" client-side it's in the wrong crate.
- WS always `peek=true` — never consume the identity's inbox.
- HTTP via `tauri-plugin-http`, WS via `tauri-plugin-websocket` (CORS + header
  auth — don't swap to raw fetch/WebSocket).
- Wire types in `api.ts` mirror `kore-protocol`; protocol changes land there
  first — update `api.ts` when a DTO the app uses changes.
- New UI = `src/components/centaury/` first, then the 7 libraries (DESKTOP-APP.md policy).
- Colors come from tokens (`bg-card`, `bg-muted`, `text-foreground`…), never fixed
  whites or hexes, so both themes work. `--accent` is the brand color, not a hover.
- Screens must fit the 1100×740 window; check them in demo mode ("Try demo mode").
- The project on screen is the project of the active token: switch through
  `Shell.openProject()` / `store.activate()`, never by changing only the title.
- Every send goes through `useChatFeed().send`: pending → sent (POST `id`) or failed
  with the text kept and a manual retry. Never auto-resend (the server is not idempotent)
  and never wait for the WS to echo your own message (peek does not send it).
- Remote screens show loading, empty and error+retry as three different states
  (`kore/QueryState.tsx`); do not swallow an exception into an empty screen.
- A conversation that got messages while you were elsewhere shows it (row badge, the
  sidebar count on Home, the "N new messages" button) and one click reaches the message.
  Nothing is marked read unless it was on screen with the window focused.
- Global does not stand in for threads or direct messages, and the composer always
  says who will receive the message before it is sent.
- One composer per audience. A second place that sends (AgentDetail's "Message") goes
  through the same `api.send`/feed path; never add a parallel send box to a screen.
- Presence says only what the server knows: "Connected" / "Not listening" plus the
  agent's own `status_context`. Never present connected as ready, or inactive as dead.
- No figure without real data behind it. If the request fails or the server has no
  such number, leave the card out — no "unavailable" placeholders, no filler charts.
- Destructive or privilege actions (kill, disable, role, password reset) open a `Dialog`
  that names the target and the consequence, and show the result (including the
  server's own refusal text, e.g. the 409 for the last admin). No `confirm()`/`prompt()`.
- Every flow works at 1100×740 in light and dark, by keyboard, with visible focus —
  dialogs and scrolling lists included.
- Settings only lists what exists: a shortcut that is not implemented or a preference
  the server cannot store is not shown. Local preferences live in `lib/prefs.ts`.

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
