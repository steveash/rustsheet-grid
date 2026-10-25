//! Viewport server over the rustsheet engine.
//!
//! The browser never holds the workbook. It subscribes to the rectangle on screen and receives
//! tiles of render-ready cells. This first slice renders a tile's display text through the
//! engine's public facade; styles, conditional formatting and the WebSocket protocol follow
//! (see `docs/architecture.md`).
#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod tile;

pub use tile::{render_tile, Tile, TileError, TileRect, MAX_TILE_CELLS};
