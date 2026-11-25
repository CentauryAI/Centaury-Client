# components/centaury — the house style, vendored

A copy of the parts of `@centaury/ui` (the Centaury design system, extracted
from the CentauryAI website) that an app needs: shell, auth, charts and a few
primitives. Copied from design-system `dd44e38`. The styles live next door in
`src/styles/centaury/` and are imported by `src/index.css`.

It is a copy, not a dependency, because the package targets Next 16 (it
imports `next/link` and `next/font`) and has no remote yet, and this repo has
to build on its own in CI.

Usage rules are the package's `RULES.md`: white ground, one accent, slate
never black, `rounded-[28px]` shells, hand-built charts, one motion curve.

## What differs from the package

| File | Change |
|---|---|
| `shell/AppShell.tsx` | Fills the window below the title bar (no `min-h-screen`, no 1180px cap); the sidebar stays put and only the content scrolls |
| `shell/Sidebar.tsx` | No router: items are buttons (`onClick`), not `next/link`. Adds `section` (group eyebrow, as in the web `/org` panel), `userAction` and a per-item `badge` count. `SidebarPromo.cta` takes `onClick` |
| `auth/AuthShell.tsx` | Fills the window; tighter gutters |
| `primitives/` | Only `Button`, `Eyebrow`, `CountUp`, `Reveal` |
| everywhere | `framer-motion` → `motion/react`; `"use client"` removed; `bg-white` → `bg-card` so the app's dark theme works |

Not copied: `chrome/` (marketing navbar and footer), `shaders/` (three.js),
`iso/`, the rest of `primitives/`, and `ui/` (this app has its own shadcn
primitives in `components/ui`).

## Tokens to know

- `--accent` is the **brand hue** (`#777fa8`: charts, dots, halos), not a hover
  surface as in stock shadcn. Hover and soft fills use `muted` / `secondary`.
- `--primary` is the brand color for text and buttons. The "black" button is
  `bg-foreground`.
- Grounds: `bg-ground-app` (app), `ground-grid` (login).
- Fonts: `font-heading` (Epilogue), body (Sora). `font-mono` renders in the
  body font on purpose, like the web.

## Updating

Copy the changed file again from `design-system/src/` and reapply the row of
the table above that touches it.
