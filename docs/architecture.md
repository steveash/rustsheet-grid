# Architecture

The full plan, including the Excel-parity checklist and phases, lives in the shared plan doc.
This file records what is built and the decisions that shape the code.

## Two repos
- `steveash/rustsheet`: the engine. Unchanged by this repo; grid needs become engine tasks there.
- `steveash/rustsheet-grid` (this repo): server, display WASM, web grid, gym. Pins the engine to a
  commit; `engine-bump.yml` moves the pin nightly and reports whether the grid still builds.

## Viewport model
The browser holds only the visible cells plus a prefetch margin. The server (rustsheet) holds the
workbook and pushes render-ready tiles for the subscribed rectangle, then patches as recalculation
changes them.

Built so far:
- `grid-server::render_tile`: display text for a rectangle, through the engine's public API.
- `grid-display-wasm::formatNumber`: the engine's number formatter, for the browser.
- `grid-core::AxisGeometry`: row/column pixel geometry with sparse overrides; O(log k) both ways,
  independent of axis length (1B rows cost the same as 100).
- `grid-protocol`: the viewport protocol's message types and binary codec, specified in
  `docs/protocol.md`. No dependencies, so it also builds for wasm32: the browser will run this
  same codec rather than a TypeScript copy.

Next: expose the codec to TypeScript through WASM (generated types), the WebSocket server over
`grid-server`, and the renderer.

## Engine APIs the grid will need (to file in rustsheet)
- Styles and resolved conditional formatting per cell, through the public facade.
- Conditional formatting evaluated for a requested range instead of the whole workbook.
- General-format display text for a given column pixel width (`#####`, shortened decimals).
- Row limit beyond 1,048,576 (spec first: whole-column references, range evaluation).
