#!/bin/sh -e
# Typecheck the embedded TS plugins against their upstream SDK types — catches
# SDK drift like HC15 (opencode ≥1.17 part schema) before a live agent does.
# Dev/CI convenience, NOT wired into cargo. Needs bun or npm on PATH.
# ponytail: the generated kilo/omp variants (import+TOOL rewrite, hook.rs)
# aren't checked — the rewrite is pinned by rust tests; only sources here.
cd "$(dirname "$0")/../src/plugins"
if command -v bun >/dev/null 2>&1; then
  bun install --ignore-scripts >/dev/null
  bun x tsc --noEmit
else
  npm install --ignore-scripts --prefer-offline >/dev/null
  npx tsc --noEmit
fi
echo "plugins typecheck: OK"
