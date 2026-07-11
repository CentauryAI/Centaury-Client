# Centaury Client — Project Context

Thin client layer: connections + UI only, zero product logic (SaaS split — logic → server/).

## Structure

- `client/` — CLI/TUI (`kore-client`), Rust, MIT
- `desktop/` — GUI app (Tauri v2 + React 19 + Vite + Tailwind v4 + shadcn)

## External dependencies

- **Protocol**: `CentauryAI/Centaury-Protocol` (git dep, tag `v0.1.0`)
- **Server**: `CentauryAI/Centaury-Server` (private, AGPL/commercial)

## Per-crate docs

- `client/CLAUDE.md` — full file map, commands, identity model, hook dialects
- `desktop/CLAUDE.md` — file map, build, invariants

## Invariants

- Thin: no logic here; if a feature "works" client-side it's in the wrong crate.
- Protocol changes land in Centaury-Protocol first — update client/desktop when DTOs change.
- New UI = pull from the 7 libraries first (see desktop/CLAUDE.md policy).

## Build

```bash
cargo build -p kore-client           # CLI
cd desktop && pnpm install && pnpm build   # frontend
cd desktop && pnpm tauri dev         # full app
```
