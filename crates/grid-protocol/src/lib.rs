//! The viewport protocol between `grid-server` and the web grid.
//!
//! Message types and their binary codec, written once here and shared: the server links this
//! crate, and the browser runs the same codec compiled to WASM. The format, ordering rules and
//! limits are specified in `docs/protocol.md`; this crate implements its "Encoding" section and
//! the limits a codec can check. Meaning the codec cannot check (tile shape, ascending axis
//! indexes, `want` inside `retain`) is the receiver's job.
//!
//! Decoding is total: malformed or hostile input returns a [`DecodeError`], never panics, and no
//! length on the wire can make the decoder allocate more than the message's own size allows.
#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod message;
mod model;
mod wire;

#[cfg(test)]
mod tests;

pub use message::{
    Change, ClientMessage, Direction, EditOp, EditStatus, ErrorCode, Query, QueryResult,
    ServerMessage, MAX_CHANGES,
};
pub use model::{
    cell_flags, diagonal, font_flags, text_flags, Axis, AxisChange, AxisEntry, Border, BorderStyle,
    Cell, CellData, CellValue, DataBar, Decoration, FillPattern, HAlign, Icon, Rect, Rgba,
    SheetInfo, Style, TileBody, Underline, VAlign, Visibility, MAX_AXIS_ENTRIES, MAX_MERGES,
    MAX_RECTS, MAX_SHEETS, MAX_STYLES, MAX_TILE_CELLS,
};
pub use wire::{DecodeError, MAX_STR_BYTES};

/// Protocol major version: a client and server must agree on it.
pub const MAJOR: u32 = 1;
/// Protocol minor version.
pub const MINOR: u32 = 0;

/// Rows per tile in v1.
pub const TILE_ROWS: u32 = 64;
/// Columns per tile in v1.
pub const TILE_COLS: u32 = 32;
/// Tile bytes a v1 server sends ahead of the client's last `Ack`.
pub const ACK_WINDOW: u64 = 1 << 20;
