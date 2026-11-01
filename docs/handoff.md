# Handoff: state of rustsheet-grid as of 2026-10-25

Read this first if you are picking up this project in a new session. It summarizes the planning
conversation that produced this repo, what exists, what is decided, and what to do next.

## The goal

An Excel-parity spreadsheet grid for the web (core grid experience nearly identical to Microsoft
365 Excel, especially conditional formatting), fast enough for complex financial models. The
browser is a thin window: all workbook state and computation run server-side in the owner's
**rustsheet** engine (Rust), so the grid can scale to 1B rows while the client holds only what is
on screen. Menus and dialogs may differ from Excel; the grid itself must match.

Parity is established by a **gym**: a service that drives real Excel for the web (screenshots +
Office.js readback) and, later, our grid through the same API, so an agent can explore Excel's
behavior and turn findings into tests without the owner in the loop.

## Where things are

| What | Where |
|---|---|
| Full plan (living doc, owner reviews it) | https://claude.ai/artifact/PZvBVWSjrHoAficxKg1oy9 |
| This repo | https://github.com/steveash/rustsheet-grid |
| First PR (scaffold + Phase 0 gym) | https://github.com/steveash/rustsheet-grid/pull/1 |
| Engine repo (separate, has its own agent process) | https://github.com/steveash/rustsheet |
| Gym setup and deploy guide | `docs/gym.md` |
| What is built, engine APIs needed | `docs/architecture.md` |

## Decisions made with the owner

- **Two repos.** The engine stays in `steveash/rustsheet`, untouched by grid work: its many agents,
  CI, merge queue and rules must not be affected by TypeScript/WASM tooling. This repo pins the
  engine by commit (`Cargo.toml`); `.github/workflows/engine-bump.yml` moves the pin nightly. Grid
  needs from the engine are filed as tasks in the rustsheet repo and follow its process
  (`CLAUDE.md` there: oracle evidence, `make check`, reviewer agents).
- **Parity target:** Excel for the web first; desktop core grid behavior is assumed near-identical.
- **Frontend:** grid core in framework-free TypeScript (canvas renderer, viewport store, input
  state machine); React only as a thin wrapper and for the app shell.
- **Rendering:** Canvas2D first, layered; DOM overlay only for the in-cell editor / formula bar.
- **Shared logic in Rust:** number formatting runs in the browser as WASM built from the engine's
  own formatter (`crates/grid-display-wasm`), because General format and `#####` depend on
  column pixel width.
- **Collaboration:** later, probably CRDT. Now: stable ids for rows/columns/cells and every edit
  as an op in a server-side log.
- **Limits:** 16,384 columns (Excel's); rows target 1B (engine currently caps at 1,048,576; needs
  an engine spec).
- **Gym host:** the owner's Linux box, published with Tailscale Funnel. Deploys are pull-based (a
  systemd timer polls `main`) because both repos are **public** and a self-hosted GitHub runner
  would let fork PRs run code on the box. Canary-then-swap with automatic rollback.
- **Gym stays thin:** primitives only (reset, Office.js ops incl. `eval`, input, observe). Probe
  logic lives with the agent, so the deployed gym rarely changes.

## What exists (PR #1, CI green including the wasm32 build)

- `crates/grid-server`: `render_tile` (display text of a rectangle via the engine facade).
- `crates/grid-display-wasm`: `formatNumber(value, format)` exported with wasm-bindgen.
- `web/packages/grid-core`: `AxisGeometry` (row/column pixel geometry, sparse overrides,
  O(log k) both directions, tested at 1B rows).
- `gym/bridge-addin`: Office.js task pane + ops (`reset`, `loadXlsx`, `setValues`, `setFormulas`,
  `setNumberFormat`, `setFormat`, `addConditionalFormat`, `setSize`, `freeze`, `select`, `read`,
  `geometry`, `selection`, `eval`). Connects to its gym over `ws://127.0.0.1:8787`.
- `gym/server`: MCP server (`gym_status`, `excel_reset`, `excel_bridge`, `excel_act`,
  `excel_observe`) over Playwright; Microsoft sign-in with TOTP; magenta-marker screen
  calibration; frame-stability waits. All routes except `/healthz` are under `/s/<GYM_TOKEN>/`.
- `gym/deploy`: Dockerfile, `deploy.sh` (canary on :8788, swap to :8787), `updater.sh` +
  `install.sh` (systemd timer).
- CI (`.github/workflows/ci.yml`): per-area jobs selected by changed paths, `ci-ok` aggregate.

**Never verified:** anything against real Excel or Docker. Expect fixes in `login.ts` (Microsoft's
sign-in pages), add-in loading (the `ws://127.0.0.1` hop from the add-in iframe relies on Chromium
flags in `excel.ts`), and calibration (task pane or promo dialogs covering the grid).

## Owner's manual steps (status unknown at handoff; ask)

Listed in the plan doc under "What you need to do manually" and in `docs/gym.md`: set default
branch to `main` (GitHub made `agent/scaffold-and-gym` the default because it was pushed first),
merge PR #1, allow Actions to create PRs, create the M365 automation user (TOTP MFA) and
`gym.xlsx`, set up the box (Docker, Tailscale Funnel on 8787, `/etc/rustsheet-gym/gym.env`,
`install.sh`), upload the add-in manifest, add the gym MCP URL as a connector.

## Next steps, in order

1. **Phase 0 gate:** once the gym is up, run `gym_status deep=true`, `excel_reset`, a data-bar
   probe, and `excel_observe`; fix what breaks. Gate: same screenshot and readback 5 runs in a row.
2. **Scrolled views:** calibration assumes A1 at the top-left; detect the scroll origin so
   `excel_act`/`excel_observe` work after Ctrl+Arrow and scrolling.
3. **Viewport protocol**: spec written (`docs/protocol.md`, v1.0 draft) and implemented as
   `crates/grid-protocol` (types + codec, tested). Next: export the codec to the browser via WASM
   with generated TypeScript types, then the WebSocket server that speaks it.
4. **Engine tasks in rustsheet** (via its `planner` agent / task process, not edited from here):
   styles and resolved CF per cell through the public facade; CF evaluated for a requested range;
   width-aware General display; >1,048,576 rows spec.
5. **Phase 1:** WebSocket server over `grid-server`, canvas renderer in `grid-core`, then the
   parity tiers T1→T5 (plan doc checklist), each driven by gym probes recorded in `docs/excel-ui/`.

## Environment notes for cloud sessions

- `rust-toolchain.toml` pins 1.97 with the wasm32 target. If `static.rust-lang.org` is not
  reachable, rustup crashes on any cargo command in this repo; work around with
  `RUSTUP_TOOLCHAIN=stable` (wasm builds then only run in CI).
- Node 22 runs the gym server's `.ts` files directly (type stripping): no build step, and only
  erasable TypeScript syntax (no enums or parameter properties; `erasableSyntaxOnly` enforces it).
- Playwright is pinned to 1.63.0 to match the browsers preinstalled in cloud sessions and the
  `mcr.microsoft.com/playwright:v1.63.0-noble` image.
- Repo settings cannot be changed through the session's GitHub proxy; ask the owner.
- Commits must use `git config user.email noreply@anthropic.com` / `user.name Claude`.
