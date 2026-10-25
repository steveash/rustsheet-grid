# rustsheet-grid — agent operating manual

This repo builds an Excel-parity web grid over the rustsheet engine, plus the gym that checks
parity against real Microsoft 365 Excel. The engine lives in a separate repo
(`steveash/rustsheet`) and is consumed here as a pinned git dependency.

**New session? Read `docs/handoff.md` first**: goals, decisions, current state and next steps.

## Prime directives
1. **Excel is the spec.** Grid behavior (painting, selection, keyboard, conditional formatting) is
   decided by gym evidence from real Excel for the web, never by memory or documentation. Record
   every finding in `docs/excel-ui/` with the probe that produced it.
2. **Never change the engine from here.** If the grid needs something from rustsheet (a new API,
   a fix), file a task in the rustsheet repo and follow its process. This repo only bumps the pin.
3. **Areas stay independent.** Each area has its own `make check-<area>` and its own CI job.
   A change in one area must not require another area's toolchain unless it crosses the boundary
   (the display crate feeds the web packages).
4. **One definition per concept.** Shared types are defined in Rust and generated for TypeScript;
   never hand-write a second copy.
5. **The gym stays thin.** The gym server exposes primitives (reset, bridge ops, act, observe).
   Probe logic, catalogs and diffing live with the agent or in tests, so the deployed gym rarely
   changes. Deploys are automatic from `main` (see `docs/gym.md`); never ask the owner to deploy.
6. **Secrets never enter the repo or a chat.** Gym credentials live only on the gym host.

## Commands
```
make check         # all areas
make check-server  # cargo fmt --check, clippy -D warnings, tests
make check-wasm    # cargo build of grid-display-wasm for wasm32-unknown-unknown
make check-web     # pnpm typecheck + vitest for web/
make check-gym     # pnpm typecheck + vitest for gym/
```

## Conventions
- Rust: pedantic clippy, no `unwrap`/`expect`/`panic` outside tests, doc comments on public items.
- TypeScript: strict mode, ES modules, Node 22, pnpm (version pinned in `package.json`).
- Commit messages: imperative summary; small, focused PRs.
