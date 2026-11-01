# Viewport protocol

How the browser grid and `grid-server` talk. The browser holds only what is on screen plus a
prefetch margin; the server holds the workbook and sends render-ready **tiles**, then **patches**
them as edits and recalculation change them.

Status: **v1.0 draft**. Implemented by `crates/grid-protocol` (types and codec); the server and
client that speak it come next. Anything marked _pending engine API_ needs an engine task in
`steveash/rustsheet` before the server can fill it in (see `docs/architecture.md`).

## Design goals

1. **Scroll-rate traffic.** Binary frames, no JSON. A 64 × 32 tile of a typical model is a few KB.
2. **Never paint a mix of revisions.** Every frame the client paints shows exactly one workbook
   revision. Ordering rules below make this hold without client-side buffering.
3. **The server decides what the client holds.** Tile eviction is deterministic and happens at a
   known point in the message stream on both sides, so the server never skips a tile the client
   threw away.
4. **One definition.** Message types and the codec are written once, in Rust
   (`crates/grid-protocol`). The browser uses the same codec compiled to WASM; TypeScript types
   are generated from it, never hand-written.
5. **Untrusted input both ways.** Every length is bounded before allocation; decoding never panics.

## Transport

- One WebSocket per workbook session. Each binary WebSocket message is exactly one protocol
  message. Text messages are a protocol error.
- Servers should offer `permessage-deflate`; the cell encoding is simple and relies on it to
  squeeze repetition (runs of the same style, repeated text).
- The first message from the client is `Hello`; the first from the server is `Welcome`. A major
  version mismatch ends the session with `Error { code: version }`.

## Concepts

### Coordinates
All rows, columns and sheet ids are 0-based unsigned integers. Rows go up to 1B (the engine caps
at 1,048,576 today, _pending engine API_), columns to 16,384. A `Rect` is `top, left, rows, cols`
on one sheet. Sheet ids are the engine's stable `SheetId`s, not positions; renaming or moving a
sheet keeps its id.

### Revision
A `u64` that increases with every committed change to the workbook (edit, undo, recalc of
volatile functions). Every tile and update carries the revision it shows.

### Tiles
The sheet is cut into a fixed lattice of tiles, `tile_rows × tile_cols` cells, announced in
`Welcome` (v1: 64 × 32). Tile `(tr, tc)` covers rows `tr·tile_rows ..` and columns
`tc·tile_cols ..`. A fixed lattice means a tile is cached by key, independent of where the
viewport happens to sit. Data tiles are unrelated to the renderer's texture tiles, whose size
is in pixels.

Inside a tile, cell `(r, c)` (relative) has index `r·tile_cols + c`.

### Styles
Cells reference an **effective style id**: the cell's own formatting with conditional formatting
already applied (fill, font, borders, number format). Styles travel once in an append-only table
per session (`Styles`); id 0 is the workbook's Normal style. Data bars and icon sets cannot be
expressed as a style and travel per cell as **decorations**.

Colors are resolved on the server (theme + tint, automatic colors) to RGBA bytes.

A color scale can mint a style per cell. If a session's table would pass `MAX_STYLES` (65,536),
the server ends the session with `Error { code: reset }` and the client reconnects with an empty
table.

### Geometry
Column widths and row heights are integer CSS pixels at 100% zoom; the client scales for zoom and
device pixel ratio. Each axis is a default size plus sparse overrides (`Axis`), the shape
`grid-core`'s `AxisGeometry` stores. A size of 0 means hidden. How Excel rounds sizes at other
zoom levels is a gym question; record the answer in `docs/excel-ui/`.

## Session flow

```
client                                   server
  Hello ───────────────────────────────▶
        ◀─────────────────────────────── Welcome, Sheets, Styles(0..), Axis(rows), Axis(cols)
  Subscribe(seq 1, sheet, want, retain) ▶
        ◀─────────────────────────────── SubscribeAck(1), Styles?, Tile, Tile, ...
  Ack(server seq) ─────────────────────▶          (flow control, about once per frame)
  Edit(req 7, base rev 41, SetInput) ──▶
        ◀─────────────────────────────── Styles?, Update(rev 42), EditResult(req 7, rev 42)
  Query(req 8, JumpEdge) ──────────────▶
        ◀─────────────────────────────── QueryResult(req 8, Cell)
```

### Subscription and the held set

`Subscribe { seq, sheet, want, retain }` names what the client is looking at. `want` and `retain`
are lists of up to 8 rectangles each (frozen panes give up to four `want` areas). `want` is what
it needs now (visible area plus prefetch margin); `retain` is what it is willing to keep cached and
must contain `want`.

The server keeps the **held set**: the tiles it has sent and not evicted. When the server processes
`Subscribe` with sequence `s`, it:

1. evicts every held tile that does not intersect `retain` (or is on another sheet),
2. sends `SubscribeAck { seq: s }`,
3. sends a `Tile` for every tile that intersects `want` and is not held, nearest the first `want`
   rectangle's center first, adding each to the held set as it is sent.

The client applies the **same eviction rule when it processes `SubscribeAck(s)`**, not when it
sends `Subscribe`. Tiles that arrive between sending `Subscribe` and receiving its ack were sent
under the old subscription and are kept until the ack. Both sides therefore agree on the held set
at every point in the stream.

A client that scrolls quickly sends a new `Subscribe` per frame. The server may coalesce: when it
reads several queued subscriptions it may process only the last (it still acks the last `seq`
only; the client treats an ack for `s` as covering every earlier one). Tiles for an abandoned
subscription that are still queued and unsent are dropped.

### Revisions and consistency

The stream is ordered, and the server keeps one rule: **after every message, every held tile is at
the newest revision the server has sent.** Concretely:

- A commit that changes the workbook produces exactly one `Update { revision }` covering every
  held tile it touches: cell patches, whole replacement tiles (for structural changes such as
  inserted rows, sorts, merges), axis changes and sheet-list changes. Styles it introduces are
  sent in a `Styles` message just before it.
- `Tile`s sent after that `Update` are rendered at its revision or later.

So the client may apply each message as it arrives and paint between any two messages. Patches
for tiles the client does not hold (because it evicted them at an ack the server had not yet
processed) are ignored.

### Edits

`Edit { req, base_revision, op }` with ops:

| Op | Meaning |
|---|---|
| `SetInput { sheet, rect, input }` | Every cell of `rect` gets `input` exactly as typed (Enter is a 1 × 1 rect, Ctrl+Enter the selection). The engine parses it as Excel parses typed input. |
| `Clear { sheet, rect }` | Delete key: clears contents, keeps formatting. |
| `Undo`, `Redo` | Server-side history for this workbook. |

The server answers with `Update` (if anything changed) and then `EditResult { req, ... }`, in
that order, so by the time the result arrives the canonical values are already applied. The
client shows typed text as a pending local echo until then.

`base_revision` is the revision the client was showing when the user acted. v1 is single-user:
the server applies edits at its current revision, but rejects one with `stale` when a structural
change (insert/delete rows or columns, sort) landed after `base_revision`, since its coordinates
may now point elsewhere. Every accepted edit is appended to the session's op log with both
revisions, which is the hook for transforming concurrent edits once collaboration arrives.

### Queries

Request/response, matched by `req`:

| Query | Result |
|---|---|
| `CellDetail { sheet, row, col }` | What the formula bar shows: the cell's input (formula with `=`, or the value as typed). |
| `JumpEdge { sheet, row, col, direction }` | Ctrl+Arrow target cell (needs the whole sheet, so it is server-side). |
| `UsedRange { sheet }` | Bounds for Ctrl+End, or none for an empty sheet. |

### Flow control

The server numbers its messages 1, 2, 3, … (implicitly; the number is not on the wire). The client
sends `Ack { seq }` with the number of the last message it has applied, about once per animation
frame. The server stops sending `Tile` messages while the bytes of tiles numbered after the last
ack exceed `ack_window` (from `Welcome`; v1: 1 MiB). Other messages are never held back. This
keeps a slow client from queueing seconds of tiles it will scroll past.

## Encoding

Little-endian. Each message starts with a one-byte tag; client tags are `0x01..`, server tags
`0x81..`. A message must be consumed exactly: trailing bytes are an error.

| Notation | Encoding |
|---|---|
| `u8` | 1 byte |
| `uv` | unsigned LEB128, at most 10 bytes. Revisions, sequence numbers, request ids and `ack_window` are 64-bit; every other `uv` (rows, columns, sheet ids, sizes, style ids, counts) must fit in 32 bits |
| `f32`, `f64` | IEEE 754, little-endian |
| `bool` | `u8`, 0 or 1 (other values are an error) |
| `str` | `uv` byte length, then UTF-8 (≤ 1 MiB) |
| `rgba` | 4 bytes: R, G, B, A |
| `list<T>` | `uv` count, then the items; a count larger than the remaining bytes is an error before any allocation |
| `opt<T>` | `bool`, then `T` if 1 |
| `Rect` | `top uv, left uv, rows uv, cols uv` |

### Client messages

| Tag | Message | Fields |
|---|---|---|
| `0x01` | `Hello` | `major uv, minor uv, client str` |
| `0x02` | `Subscribe` | `seq uv, sheet uv, want list<Rect>, retain list<Rect>` (each list 1 to 8 rects) |
| `0x03` | `Ack` | `seq uv` |
| `0x04` | `Edit` | `req uv, base_revision uv, op EditOp` |
| `0x05` | `Query` | `req uv, query Query` |

`EditOp`: `u8` tag, then `1 SetInput { sheet uv, rect Rect, input str }`, `2 Clear { sheet uv,
rect Rect }`, `3 Undo`, `4 Redo`.

`Query`: `u8` tag, then `1 CellDetail { sheet uv, row uv, col uv }`, `2 JumpEdge { sheet uv, row
uv, col uv, direction u8 }` (0 up, 1 down, 2 left, 3 right), `3 UsedRange { sheet uv }`.

### Server messages

| Tag | Message | Fields |
|---|---|---|
| `0x81` | `Welcome` | `major uv, minor uv, revision uv, tile_rows uv, tile_cols uv, max_rows uv, max_cols uv, ack_window uv` |
| `0x82` | `Sheets` | `revision uv, sheets list<SheetInfo>` (the whole list, in tab order) |
| `0x83` | `Styles` | `first_id uv, styles list<Style>` (`first_id` must equal the client's table length) |
| `0x84` | `Axis` | `AxisChange` |
| `0x85` | `SubscribeAck` | `seq uv` |
| `0x86` | `Tile` | `revision uv, tile TileBody` |
| `0x87` | `Update` | `revision uv, changes list<Change>` |
| `0x88` | `EditResult` | `req uv, revision uv, status u8` (0 ok, 1 stale, 2 rejected)`, message str` |
| `0x89` | `QueryResult` | `req uv, result QueryResult` |
| `0x8A` | `Error` | `code u8` (0 protocol, 1 version, 2 reset, 3 internal)`, message str`; the server closes after it. On `reset` the client reconnects with empty state. |

`SheetInfo`: `id uv, name str, visibility u8` (0 visible, 1 hidden, 2 very hidden)`, frozen_rows
uv, frozen_cols uv, show_gridlines bool, default_row_height uv, default_col_width uv`.

`AxisChange`: `sheet uv, axis u8` (0 rows, 1 columns)`, replace bool, entries list<{ index uv,
size uv }>`. Indexes ascending and unique. With `replace`, the entries are the axis's complete
override set; otherwise each entry sets one index, and an entry equal to the default size removes
the override.

`TileBody`: `sheet uv, tile_row uv, tile_col uv, merges list<Rect>, cells list<Cell>`. `merges`
are the merged ranges that intersect the tile, in sheet coordinates (a merge may be larger than
the tile).

`Change`: `u8` tag, then `1 Patch { sheet uv, tile_row uv, tile_col uv, cells list<Cell> }`,
`2 Tile { TileBody }` (replaces the held tile), `3 Axis { AxisChange }`, `4 Sheets { sheets
list<SheetInfo> }`.

`QueryResult`: `u8` tag, then `0 Failed { message str }`, `1 CellDetail { input str }`,
`2 Cell { row uv, col uv }`, `3 Range { range opt<Rect> }`.

### Cells

A cell list holds only cells that are not default (no value, style 0, no flags), sorted by index.
Each record:

```
index_delta uv      index − previous index − 1 (the first record's previous index is −1)
kind        u8      0 blank, 1 number, 2 text, 3 bool, 4 error, 5 cleared
-- the rest is absent for `cleared` (patches only: the cell is now default)
flags       u8      see below
style       uv      effective style id
text        str     kinds 1-4: display text at unlimited width
number      f64     kind 1 only: the value
decoration          only if flags has DECORATED
```

Flags: `0x01` note, `0x02` threaded comment, `0x04` list validation (dropdown arrow when active),
`0x08` error indicator (green triangle), `0x10` spill anchor, `0x20` inside a spill range,
`0x40` decorated, `0x80` hide text (CF "show bar/icon only").

`text` is what the cell shows when the column is wide enough. Width-dependent display (General's
shortening, `#####`, `*` repeat fill, `_` padding) is applied by the client with the engine's
formatter compiled to WASM, using `number`, the style's number format and the measured width.

Decoration: `parts u8` (`0x01` data bar, `0x02` icon), then if data bar: `start f32, end f32` (bar
extent as fractions of the cell's inner width, `start < end`; the axis, if any, is the edge nearest
the zero point), `fill rgba, border rgba, bar_flags u8` (`0x01` gradient, `0x02` has border,
`0x04` axis drawn, `axis f32` follows when set); then if icon: `set u8, index u8` (OOXML icon set
order). Exact Excel geometry (insets, minimum length, axis dash pattern) is decided by the gym and
belongs to the client renderer, not the wire.

### Style

```
font_name     str
font_size     uv      hundredths of a point
font_flags    u8      0x01 bold, 0x02 italic, 0x04 strikethrough, 0x08 superscript, 0x10 subscript
underline     u8      0 none, 1 single, 2 double, 3 single accounting, 4 double accounting
font_color    rgba
fill_pattern  u8      OOXML ST_PatternType order (0 none, 1 solid, 2 mediumGray, ...)
fill_fg       rgba
fill_bg       rgba
h_align       u8      0 general, 1 left, 2 center, 3 right, 4 fill, 5 justify,
                      6 center across selection, 7 distributed
v_align       u8      0 bottom, 1 center, 2 top, 3 justify, 4 distributed
text_flags    u8      0x01 wrap, 0x02 shrink to fit
indent        uv
rotation      u8      OOXML textRotation: 0-90 up, 91-180 down, 255 stacked
border_left, border_right, border_top, border_bottom, border_diagonal:
              u8 line style (OOXML ST_BorderStyle order, 0 none) + rgba
diagonal      u8      0x01 up, 0x02 down
number_format str     the format code; "General" for General
```

Border precedence between neighbors is resolved by the client renderer (it has both cells), so
the client must subscribe at least one cell beyond what it paints on every side; the prefetch
margin does this.

## Limits

| Limit | v1 | Enforced by |
|---|---|---|
| Message size | 16 MiB | transport |
| String length | 1 MiB | codec |
| Rectangles per `want` / `retain` | 8 | codec |
| Cells per tile | `tile_rows × tile_cols` (≤ 4,096) | codec (≤ 4,096) and receiver (tile shape) |
| Merges per tile | 4,096 | codec |
| Styles per session | 65,536 | server (`Error { reset }`) |
| Axis entries per message | 1,048,576 | codec; larger override sets are split into one `replace` plus merges |
| Sheets per listing | 65,536 | codec |
| Styles per `Styles` message | 65,536 | codec |
| Changes per `Update` | 65,536 | codec |

A receiver also checks meaning the codec cannot: indexes inside the tile, ascending axis indexes,
`want` inside `retain`, style ids that exist, rectangles inside the sheet.

## Pending

- **Overflow context** (_pending engine API_). Text overflows into empty neighbors, possibly from
  a cell far outside the subscription (long text in A1 while scrolled to column F). Planned:
  for each subscribed row, the nearest non-empty cell beyond each edge of the subscription, sent
  with the tiles. Needs an engine query for "nearest stored cell in this row from column c".
- **Styles and CF from the engine** (_pending engine API_): the facade exposes number formats only
  today, so until it exposes the rest, the server's styles differ only in their number format.
- **Rows beyond 1,048,576** (_pending engine spec_).
- **Collaboration.** Positional coordinates plus `base_revision` and the op log now; stable row and
  column ids or a CRDT later, as a protocol v2.
