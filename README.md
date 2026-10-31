# rustsheet-grid

An Excel-parity spreadsheet grid for the web, backed by the
[rustsheet](https://github.com/steveash/rustsheet) engine running in the cloud.

The browser is a thin viewport: it holds only the cells on screen and paints them on a canvas.
The workbook, recalculation, conditional formatting and undo all live in rustsheet on the server.
Excel parity is checked against real Microsoft 365 Excel by the **gym**, a service that drives
Excel for the web and our grid through the same API and diffs them cell by cell.

## Layout

| Path | Toolchain | What |
|---|---|---|
| `crates/grid-server` | Rust (Cargo) | Viewport server over the rustsheet engine |
| `crates/grid-display-wasm` | Rust → wasm32 | Number formatting shared with the browser |
| `web/packages/grid-core` | TypeScript (pnpm) | Framework-free canvas grid |
| `gym/server` | TypeScript (pnpm) | Excel oracle: Playwright + MCP primitives |
| `gym/bridge-addin` | Office.js | Add-in that sets and reads state inside Excel |
| `gym/deploy` | Docker, bash | Self-updating deploy for the gym host |
| `docs/` | | Architecture, protocol and gym setup |

The engine is a pinned git dependency (`rustsheet = { git, rev }` in `Cargo.toml`). A nightly
workflow bumps the pin and runs this repo's CI, so engine changes reach the grid within a day and
never block work in the engine repo.

## Commands

```
make check          # everything
make check-server   # Rust: fmt, clippy, tests
make check-wasm     # Rust → wasm32 build of the display crate
make check-web      # grid packages: typecheck, tests
make check-gym      # gym packages: typecheck, tests
```

Gym setup and deployment: [docs/gym.md](docs/gym.md).
