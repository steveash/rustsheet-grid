//! Client and server messages (docs/protocol.md, "Encoding").

use crate::model::{
    read_cells, read_list, write_cells, write_list, AxisChange, Cell, Codec, Rect, SheetInfo,
    Style, TileBody, MAX_RECTS, MAX_SHEETS, MAX_STYLES,
};
use crate::wire::{wire_enum, DecodeError, Reader, Writer};

/// Most changes in one `Update`.
pub const MAX_CHANGES: usize = 1 << 16;

wire_enum! {
    /// Ctrl+Arrow direction.
    pub enum Direction ("direction") {
        /// Ctrl+Up.
        Up = 0,
        /// Ctrl+Down.
        Down = 1,
        /// Ctrl+Left.
        Left = 2,
        /// Ctrl+Right.
        Right = 3,
    }
}

/// A change the user makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditOp {
    /// Every cell of `rect` gets `input` exactly as typed.
    SetInput {
        /// Sheet id.
        sheet: u32,
        /// Target cells (1 × 1 for Enter, the selection for Ctrl+Enter).
        rect: Rect,
        /// What the user typed.
        input: String,
    },
    /// Clear contents, keep formatting (Delete key).
    Clear {
        /// Sheet id.
        sheet: u32,
        /// Target cells.
        rect: Rect,
    },
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
}

impl Codec for EditOp {
    fn write(&self, w: &mut Writer) {
        match self {
            Self::SetInput { sheet, rect, input } => {
                w.u8(1);
                w.u32v(*sheet);
                rect.write(w);
                w.str(input);
            }
            Self::Clear { sheet, rect } => {
                w.u8(2);
                w.u32v(*sheet);
                rect.write(w);
            }
            Self::Undo => w.u8(3),
            Self::Redo => w.u8(4),
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            1 => Self::SetInput {
                sheet: r.u32v()?,
                rect: Rect::read(r)?,
                input: r.str()?,
            },
            2 => Self::Clear {
                sheet: r.u32v()?,
                rect: Rect::read(r)?,
            },
            3 => Self::Undo,
            4 => Self::Redo,
            value => {
                return Err(DecodeError::BadTag {
                    what: "edit op",
                    value,
                })
            }
        })
    }
}

/// A question only the server can answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Query {
    /// What the formula bar shows for a cell.
    CellDetail {
        /// Sheet id.
        sheet: u32,
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// Where Ctrl+Arrow from a cell lands.
    JumpEdge {
        /// Sheet id.
        sheet: u32,
        /// Row.
        row: u32,
        /// Column.
        col: u32,
        /// Arrow pressed.
        direction: Direction,
    },
    /// The sheet's used range (Ctrl+End).
    UsedRange {
        /// Sheet id.
        sheet: u32,
    },
}

impl Codec for Query {
    fn write(&self, w: &mut Writer) {
        match *self {
            Self::CellDetail { sheet, row, col } => {
                w.u8(1);
                w.u32v(sheet);
                w.u32v(row);
                w.u32v(col);
            }
            Self::JumpEdge {
                sheet,
                row,
                col,
                direction,
            } => {
                w.u8(2);
                w.u32v(sheet);
                w.u32v(row);
                w.u32v(col);
                w.u8(direction.to_u8());
            }
            Self::UsedRange { sheet } => {
                w.u8(3);
                w.u32v(sheet);
            }
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            1 => Self::CellDetail {
                sheet: r.u32v()?,
                row: r.u32v()?,
                col: r.u32v()?,
            },
            2 => Self::JumpEdge {
                sheet: r.u32v()?,
                row: r.u32v()?,
                col: r.u32v()?,
                direction: Direction::from_u8(r.u8()?)?,
            },
            3 => Self::UsedRange { sheet: r.u32v()? },
            value => {
                return Err(DecodeError::BadTag {
                    what: "query",
                    value,
                })
            }
        })
    }
}

/// A message from the browser to the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientMessage {
    /// First message of a session.
    Hello {
        /// Protocol major version.
        major: u32,
        /// Protocol minor version.
        minor: u32,
        /// Client name and version, for logs.
        client: String,
    },
    /// What the client is looking at.
    Subscribe {
        /// Increases with every subscription.
        seq: u64,
        /// Sheet id.
        sheet: u32,
        /// Needed now: visible area plus prefetch margin (1 to [`MAX_RECTS`] rects).
        want: Vec<Rect>,
        /// Kept if already held; must contain `want` (1 to [`MAX_RECTS`] rects).
        retain: Vec<Rect>,
    },
    /// The last server message the client has applied.
    Ack {
        /// Server message number.
        seq: u64,
    },
    /// A change.
    Edit {
        /// Request id, echoed in `EditResult`.
        req: u64,
        /// The revision the client showed when the user acted.
        base_revision: u64,
        /// The change.
        op: EditOp,
    },
    /// A question.
    Query {
        /// Request id, echoed in `QueryResult`.
        req: u64,
        /// The question.
        query: Query,
    },
}

/// Reads a rectangle list of 1 to [`MAX_RECTS`] items.
fn read_rects(r: &mut Reader<'_>, what: &'static str) -> Result<Vec<Rect>, DecodeError> {
    let rects: Vec<Rect> = read_list(r, what, MAX_RECTS)?;
    if rects.is_empty() {
        return Err(DecodeError::TooMany { what, count: 0 });
    }
    Ok(rects)
}

impl ClientMessage {
    /// Encodes the message as one binary WebSocket frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Self::Hello {
                major,
                minor,
                client,
            } => {
                w.u8(0x01);
                w.u32v(*major);
                w.u32v(*minor);
                w.str(client);
            }
            Self::Subscribe {
                seq,
                sheet,
                want,
                retain,
            } => {
                w.u8(0x02);
                w.uv(*seq);
                w.u32v(*sheet);
                write_list(&mut w, want);
                write_list(&mut w, retain);
            }
            Self::Ack { seq } => {
                w.u8(0x03);
                w.uv(*seq);
            }
            Self::Edit {
                req,
                base_revision,
                op,
            } => {
                w.u8(0x04);
                w.uv(*req);
                w.uv(*base_revision);
                op.write(&mut w);
            }
            Self::Query { req, query } => {
                w.u8(0x05);
                w.uv(*req);
                query.write(&mut w);
            }
        }
        w.into_bytes()
    }

    /// Decodes one binary WebSocket frame.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let message = match r.u8()? {
            0x01 => Self::Hello {
                major: r.u32v()?,
                minor: r.u32v()?,
                client: r.str()?,
            },
            0x02 => Self::Subscribe {
                seq: r.uv()?,
                sheet: r.u32v()?,
                want: read_rects(&mut r, "want rects")?,
                retain: read_rects(&mut r, "retain rects")?,
            },
            0x03 => Self::Ack { seq: r.uv()? },
            0x04 => Self::Edit {
                req: r.uv()?,
                base_revision: r.uv()?,
                op: EditOp::read(&mut r)?,
            },
            0x05 => Self::Query {
                req: r.uv()?,
                query: Query::read(&mut r)?,
            },
            value => {
                return Err(DecodeError::BadTag {
                    what: "client message",
                    value,
                })
            }
        };
        r.finish()?;
        Ok(message)
    }
}

/// One part of an `Update`.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    /// Cells of a held tile that changed.
    Patch {
        /// Sheet id.
        sheet: u32,
        /// Tile row.
        tile_row: u32,
        /// Tile column.
        tile_col: u32,
        /// Changed cells, sorted by index; `data: None` means the cell became default.
        cells: Vec<Cell>,
    },
    /// A held tile, replaced whole (structural changes).
    Tile(TileBody),
    /// Row heights or column widths.
    Axis(AxisChange),
    /// The complete sheet list, in tab order.
    Sheets(Vec<SheetInfo>),
}

impl Codec for Change {
    fn write(&self, w: &mut Writer) {
        match self {
            Self::Patch {
                sheet,
                tile_row,
                tile_col,
                cells,
            } => {
                w.u8(1);
                w.u32v(*sheet);
                w.u32v(*tile_row);
                w.u32v(*tile_col);
                write_cells(w, cells);
            }
            Self::Tile(tile) => {
                w.u8(2);
                tile.write(w);
            }
            Self::Axis(axis) => {
                w.u8(3);
                axis.write(w);
            }
            Self::Sheets(sheets) => {
                w.u8(4);
                write_list(w, sheets);
            }
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            1 => Self::Patch {
                sheet: r.u32v()?,
                tile_row: r.u32v()?,
                tile_col: r.u32v()?,
                cells: read_cells(r)?,
            },
            2 => Self::Tile(TileBody::read(r)?),
            3 => Self::Axis(AxisChange::read(r)?),
            4 => Self::Sheets(read_list(r, "sheets", MAX_SHEETS)?),
            value => {
                return Err(DecodeError::BadTag {
                    what: "change",
                    value,
                })
            }
        })
    }
}

wire_enum! {
    /// How an edit ended.
    pub enum EditStatus ("edit status") {
        /// Applied; its `Update` came just before.
        Ok = 0,
        /// A structural change landed after the edit's base revision; re-issue it.
        Stale = 1,
        /// Refused (bad input, protected cell, ...); the message says why.
        Rejected = 2,
    }
}

/// The answer to a [`Query`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryResult {
    /// The query could not be answered.
    Failed {
        /// Why.
        message: String,
    },
    /// The cell's input as the formula bar shows it.
    CellDetail {
        /// Formula with `=`, or the value as typed; empty for an empty cell.
        input: String,
    },
    /// A cell position.
    Cell {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// A range, or none (empty sheet).
    Range(Option<Rect>),
}

impl Codec for QueryResult {
    fn write(&self, w: &mut Writer) {
        match self {
            Self::Failed { message } => {
                w.u8(0);
                w.str(message);
            }
            Self::CellDetail { input } => {
                w.u8(1);
                w.str(input);
            }
            Self::Cell { row, col } => {
                w.u8(2);
                w.u32v(*row);
                w.u32v(*col);
            }
            Self::Range(range) => {
                w.u8(3);
                w.bool(range.is_some());
                if let Some(rect) = range {
                    rect.write(w);
                }
            }
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(match r.u8()? {
            0 => Self::Failed { message: r.str()? },
            1 => Self::CellDetail { input: r.str()? },
            2 => Self::Cell {
                row: r.u32v()?,
                col: r.u32v()?,
            },
            3 => Self::Range(if r.bool()? {
                Some(Rect::read(r)?)
            } else {
                None
            }),
            value => {
                return Err(DecodeError::BadTag {
                    what: "query result",
                    value,
                })
            }
        })
    }
}

wire_enum! {
    /// Why the server ended the session.
    pub enum ErrorCode ("error code") {
        /// The client broke the protocol.
        Protocol = 0,
        /// Incompatible major version.
        Version = 1,
        /// Session state outgrew its limits; reconnect with empty state.
        Reset = 2,
        /// Server fault.
        Internal = 3,
    }
}

/// A message from the server to the browser.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerMessage {
    /// First message of a session.
    Welcome {
        /// Protocol major version.
        major: u32,
        /// Protocol minor version.
        minor: u32,
        /// Current workbook revision.
        revision: u64,
        /// Rows per tile.
        tile_rows: u32,
        /// Columns per tile.
        tile_cols: u32,
        /// Rows in a sheet.
        max_rows: u32,
        /// Columns in a sheet.
        max_cols: u32,
        /// Tile bytes the server sends ahead of the client's last `Ack`.
        ack_window: u64,
    },
    /// The complete sheet list, in tab order.
    Sheets {
        /// Revision it shows.
        revision: u64,
        /// Sheets.
        sheets: Vec<SheetInfo>,
    },
    /// Styles appended to the session's table.
    Styles {
        /// Id of the first style; equals the client's table length.
        first_id: u32,
        /// Styles, ids `first_id ..`.
        styles: Vec<Style>,
    },
    /// Row heights or column widths.
    Axis(AxisChange),
    /// The server processed `Subscribe { seq }` (both sides evict now).
    SubscribeAck {
        /// The subscription's sequence number.
        seq: u64,
    },
    /// A tile the client did not hold.
    Tile {
        /// Revision it shows.
        revision: u64,
        /// Contents.
        tile: TileBody,
    },
    /// Everything one commit changed in the held tiles.
    Update {
        /// The new revision.
        revision: u64,
        /// What changed.
        changes: Vec<Change>,
    },
    /// How an edit ended.
    EditResult {
        /// The edit's request id.
        req: u64,
        /// The revision after the edit (unchanged if it was not applied).
        revision: u64,
        /// Outcome.
        status: EditStatus,
        /// Why, when not `Ok`.
        message: String,
    },
    /// The answer to a query.
    QueryResult {
        /// The query's request id.
        req: u64,
        /// Answer.
        result: QueryResult,
    },
    /// The session ends; the server closes after this.
    Error {
        /// Why.
        code: ErrorCode,
        /// Details, for logs.
        message: String,
    },
}

impl ServerMessage {
    /// Encodes the message as one binary WebSocket frame.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::default();
        match self {
            Self::Welcome {
                major,
                minor,
                revision,
                tile_rows,
                tile_cols,
                max_rows,
                max_cols,
                ack_window,
            } => {
                w.u8(0x81);
                w.u32v(*major);
                w.u32v(*minor);
                w.uv(*revision);
                w.u32v(*tile_rows);
                w.u32v(*tile_cols);
                w.u32v(*max_rows);
                w.u32v(*max_cols);
                w.uv(*ack_window);
            }
            Self::Sheets { revision, sheets } => {
                w.u8(0x82);
                w.uv(*revision);
                write_list(&mut w, sheets);
            }
            Self::Styles { first_id, styles } => {
                w.u8(0x83);
                w.u32v(*first_id);
                write_list(&mut w, styles);
            }
            Self::Axis(axis) => {
                w.u8(0x84);
                axis.write(&mut w);
            }
            Self::SubscribeAck { seq } => {
                w.u8(0x85);
                w.uv(*seq);
            }
            Self::Tile { revision, tile } => {
                w.u8(0x86);
                w.uv(*revision);
                tile.write(&mut w);
            }
            Self::Update { revision, changes } => {
                w.u8(0x87);
                w.uv(*revision);
                write_list(&mut w, changes);
            }
            Self::EditResult {
                req,
                revision,
                status,
                message,
            } => {
                w.u8(0x88);
                w.uv(*req);
                w.uv(*revision);
                w.u8(status.to_u8());
                w.str(message);
            }
            Self::QueryResult { req, result } => {
                w.u8(0x89);
                w.uv(*req);
                result.write(&mut w);
            }
            Self::Error { code, message } => {
                w.u8(0x8a);
                w.u8(code.to_u8());
                w.str(message);
            }
        }
        w.into_bytes()
    }

    /// Decodes one binary WebSocket frame.
    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader::new(bytes);
        let message = match r.u8()? {
            0x81 => Self::Welcome {
                major: r.u32v()?,
                minor: r.u32v()?,
                revision: r.uv()?,
                tile_rows: r.u32v()?,
                tile_cols: r.u32v()?,
                max_rows: r.u32v()?,
                max_cols: r.u32v()?,
                ack_window: r.uv()?,
            },
            0x82 => Self::Sheets {
                revision: r.uv()?,
                sheets: read_list(&mut r, "sheets", MAX_SHEETS)?,
            },
            0x83 => Self::Styles {
                first_id: r.u32v()?,
                styles: read_list(&mut r, "styles", MAX_STYLES)?,
            },
            0x84 => Self::Axis(AxisChange::read(&mut r)?),
            0x85 => Self::SubscribeAck { seq: r.uv()? },
            0x86 => Self::Tile {
                revision: r.uv()?,
                tile: TileBody::read(&mut r)?,
            },
            0x87 => Self::Update {
                revision: r.uv()?,
                changes: read_list(&mut r, "changes", MAX_CHANGES)?,
            },
            0x88 => Self::EditResult {
                req: r.uv()?,
                revision: r.uv()?,
                status: EditStatus::from_u8(r.u8()?)?,
                message: r.str()?,
            },
            0x89 => Self::QueryResult {
                req: r.uv()?,
                result: QueryResult::read(&mut r)?,
            },
            0x8a => Self::Error {
                code: ErrorCode::from_u8(r.u8()?)?,
                message: r.str()?,
            },
            value => {
                return Err(DecodeError::BadTag {
                    what: "server message",
                    value,
                })
            }
        };
        r.finish()?;
        Ok(message)
    }
}
