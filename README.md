# Centaury Client

Thin client layer for the Centaury network — connections + UI only, zero product logic.

## Structure

| Crate | Description |
|---|---|
| `client/` | CLI/TUI client (`centaury`) — Rust, MIT |
| `desktop/` | GUI desktop app (Tauri v2 + React 19 + Vite) |

## Dependencies

- **Protocol**: [`CentauryAI/Centaury-Protocol`](https://github.com/CentauryAI/Centaury-Protocol) (wire types, shared)
- **Server**: [`CentauryAI/Centaury-Server`](https://github.com/CentauryAI/Centaury-Server) (cloud backend, private)

## Philosophy

All product logic lives server-side. The client is publishable (MIT) because there's nothing to steal in it. New feature with logic → goes in `server/`, never here.

## Build

```bash
# CLI client
cargo build -p centaury

# Desktop (frontend)
cd desktop && pnpm install && pnpm build

# Desktop (full Tauri)
cd desktop && pnpm tauri dev
```

## License

MIT
