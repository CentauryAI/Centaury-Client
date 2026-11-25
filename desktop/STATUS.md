# Desktop app — status

State of the desktop redesign after the four-stage pass (chat reliability, chat
features, settings/agent/summon, home/org/network). Everything below is in the
working tree of branch `Solar2004/improve-centaury-desktop-app`; nothing is
committed yet.

The file map and the rules that came out of this work live in `CLAUDE.md`
(sections "File map" and "Invariants"). This file only tracks what is done, what
is still open, and what has to change outside this repo.

## Done

### Chat (stages 1 and 2)

- One feed and one send path per project (`lib/useChatFeed.ts`): load states,
  dedupe by message id, optimistic bubble, `sending → sent / failed`, manual
  retry and discard. A failed message keeps its text; nothing is resent
  automatically.
- Your own messages show up exactly once. The WebSocket in peek mode never
  echoes the sender, so the echo comes from the id returned by `POST /v1/messages`.
- WebSocket reconnects on its own (1 s, 2 s, 4 s … capped at 30 s) and the
  history is reloaded on reconnect and when the window regains focus. The header
  shows Connected / Reconnecting.
- One instance token per project (`register-human`), so switching project needs
  no logout and never mixes data. A 403 shows a banner on Home and the workspace
  does not open.
- Conversation list sorted by last activity, with last-message preview and
  unread badges. Unread state is local (`kore.read.<project>` in localStorage)
  and a conversation is only marked read while it is open and the window has
  focus.
- Single composer (`kore/Composer.tsx`): audience line, `@` autocomplete, reply,
  Enter / Shift+Enter.
- Markdown rendering (`kore/Markdown.tsx`, `react-markdown` + `remark-gfm`).
- Tabs for announcements, activity, DMs and threads; open a thread and reply in it.
- Timestamps and backwards pagination through
  `GET /v1/org/projects/{name}/messages`; anchored scroll, day separators,
  "N new" button.
- Shared loading / empty / error view (`kore/QueryState.tsx`); errors distinguish
  401, 403, 429, 5xx and network. Any 401 with a token sends the user to login.

### Settings, agent detail, summon (stage 3)

- Settings rewritten: Account (read-only, from `/v1/user/me`), Appearance
  (light / dark / system, Reduce motion), Notifications, Shortcuts, Connection,
  About.
- Unread badge on "Home" in the sidebar and an optional in-app toast for new
  messages (on by default).
- Agent detail: status and Message first; Kill moved to a "More" menu behind a
  confirmation dialog.
- Summon: basics on top, "Advanced" collapsed, one-line summary before the
  button; the button is disabled with a message when the tool is not installed.
- Reduce motion also disables Motion animations (`MotionConfig` in `App.tsx`).

### Home, Org, Network (stage 4)

- Home: "Continue in <project>" card with its agents, plus a row of figures
  (Projects, Members, Agents `x of max`). A figure is left out when its request
  fails; there is no chart and no placeholder metric.
- Org: tabs Accounts / Projects / Agents / Audit for admins, Members / Agents for
  members. Role change, disable and password reset go through one dialog; the
  server's 409 ("last admin") is shown as is.
- Network: legend, Graph | List toggle, "Open project" button. A project's
  roster comes from its own token, or from `/v1/org/agents` when the project was
  never opened (agents only, with a note saying so).

## Open on the client

Small, no server change needed:

- **Native run-through.** Everything was verified in a browser against the demo
  server. In the real Tauri window these are untested: About / "Check for
  updates", `kill_agent`, `spawn_launch` and the detected tool list, the native
  http and websocket plugins.
- **Member account.** Only an admin account was exercised live; the member view
  of Org was only seen in demo mode.
- **Theme "system"** was not checked on an OS that is actually in dark mode.
- **Real 401 / 403 / 429.** Handled in code, only 403 and 500 were simulated.
- **Backwards pagination** could not be checked with real history: the demo
  server returned no older page.
- **Hidden workspace stays alive.** To keep the unread count, the project
  workspace stays mounted while Home or Settings is shown, so its WebSocket and
  the 3 s roster poll keep running. Worth revisiting if it shows up in usage.
- **Tauri plugin versions.** `tauri dev` reports a Rust/npm version mismatch for
  updater, opener, websocket and http. It still runs.
- **Bundle size.** The build warns about a chunk over 500 kB; no code splitting
  was done.
- **`kore/Chat.tsx`** is not wired to anything; it was only adapted to the new
  `connectWs` signature. Delete it or reuse it.

Left out on purpose (not built, so not listed in the UI either):

- Command palette (Ctrl/Cmd+K).
- Language selector — the app is English only.
- OS-level notifications, sounds, per-channel mute, autostart. These need new
  Tauri plugins (notification, autostart).
- Traffic pulses on the Network graph.

## Needs Centaury-Server / Centaury-Protocol

Each item was checked against the server as it is today and dropped from the
client work because the server cannot back it. DTO changes go to
Centaury-Protocol first.

| Feature | What is missing |
|---|---|
| Read state per conversation, synced across devices | No route or table for read markers. `GET /v1/messages/unread` is a single global count tied to a consumer cursor, so it never moves for a peek client. |
| User preferences stored on the server | No route (`/v1/user/preferences` does not exist). Preferences live in localStorage (`kore.pref.*`). |
| Timestamp and cursor on the chat routes | `Delivery` has no timestamp and `GET /v1/messages` has no `before`. The client works around it with the org messages route, which needs a session token and lacks `sender_kind`. |
| Idempotent send | `SendRequest` carries no client id. If the server stores a message but the response is lost, Retry sends it twice. |
| Recipient preflight / pure broadcast mode | Not available; the client computes the audience locally. An `@` in the text changes the scope. |
| Edit profile, change own password | No `PATCH /v1/user/me`. Members can only use the email reset flow; `/v1/user/me` does not return the user id. |
| Org-wide traffic stream | A human's peek socket does not see agent-to-agent messages. |
| Roster of a project you have not opened, with `tool` and `owner` | `/v1/org/agents` returns neither field. |
| Typed errors | The server answers with plain text; the client maps status codes. |

Server behaviour worth knowing, not a bug:

- An instance's `status` flips to `active` on every (re)registration, so the dot
  is "registered", not "listening".
- A human broadcast is never gated with a 409; only agents are.

## How it was verified

`pnpm exec tsc --noEmit`, `pnpm build` and `cargo test --all-targets` (69 tests)
pass. The UI checks are Playwright scripts kept locally in `.playwright-mcp/`
(git-ignored: the folder also holds a reference copy of the private server).
Last full pass: stage 2 40/40, stage 3 28/28, stage 4 18/18, plus two demo
scripts and a five-check script for the final fixes.

Stage 3 failed once in three final runs (a click timed out and the unread
counter stayed at 2 after reading the conversation), then passed twice. The
cause was not found; treat the unread-clears check as possibly flaky.
