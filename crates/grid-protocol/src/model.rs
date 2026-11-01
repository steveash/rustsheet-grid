//! Data carried by messages: rectangles, sheets, geometry, styles, cells.

use crate::wire::{wire_enum, DecodeError, Reader, Writer};

/// Most rectangles in a subscription's `want` or `retain` list.
pub const MAX_RECTS: usize = 8;
/// Most cells one tile may hold.
pub const MAX_TILE_CELLS: usize = 4096;
/// Most merged ranges listed for one tile.
pub const MAX_MERGES: usize = 4096;
/// Most entries in one axis change; larger override sets are split.
pub const MAX_AXIS_ENTRIES: usize = 1 << 20;
/// Most sheets in a workbook listing.
pub const MAX_SHEETS: usize = 1 << 16;
/// Most styles in a session's table, and so in one `Styles` message.
pub const MAX_STYLES: usize = 1 << 16;

/// Encoding and decoding of one wire type.
pub(crate) trait Codec: Sized {
    fn write(&self, w: &mut Writer);
    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError>;
}

/// Writes a list: count, then items.
pub(crate) fn write_list<T: Codec>(w: &mut Writer, items: &[T]) {
    w.len(items.len());
    for item in items {
        item.write(w);
    }
}

/// Reads a list of at most `max` items.
pub(crate) fn read_list<T: Codec>(
    r: &mut Reader<'_>,
    what: &'static str,
    max: usize,
) -> Result<Vec<T>, DecodeError> {
    r.list(what, max, T::read)
}

/// A rectangle of cells, 0-based. Which sheet it is on comes from the enclosing message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    /// First row.
    pub top: u32,
    /// First column.
    pub left: u32,
    /// Number of rows.
    pub rows: u32,
    /// Number of columns.
    pub cols: u32,
}

impl Codec for Rect {
    fn write(&self, w: &mut Writer) {
        w.u32v(self.top);
        w.u32v(self.left);
        w.u32v(self.rows);
        w.u32v(self.cols);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            top: r.u32v()?,
            left: r.u32v()?,
            rows: r.u32v()?,
            cols: r.u32v()?,
        })
    }
}

/// A color resolved to RGBA bytes (theme, tint and automatic colors are resolved by the server).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba(pub [u8; 4]);

impl Codec for Rgba {
    fn write(&self, w: &mut Writer) {
        w.bytes(&self.0);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self(r.take::<4>()?))
    }
}

/// Reads a flags byte, refusing bits outside `known`.
fn read_flags(r: &mut Reader<'_>, what: &'static str, known: u8) -> Result<u8, DecodeError> {
    let value = r.u8()?;
    if value & !known == 0 {
        Ok(value)
    } else {
        Err(DecodeError::BadTag { what, value })
    }
}

wire_enum! {
    /// Whether a sheet's tab is shown.
    pub enum Visibility ("sheet visibility") {
        /// Shown.
        Visible = 0,
        /// Hidden; the user can unhide it.
        Hidden = 1,
        /// Hidden; only code can unhide it.
        VeryHidden = 2,
    }
}

/// One sheet of the workbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetInfo {
    /// The engine's stable sheet id.
    pub id: u32,
    /// Tab name.
    pub name: String,
    /// Whether the tab is shown.
    pub visibility: Visibility,
    /// Rows frozen at the top.
    pub frozen_rows: u32,
    /// Columns frozen at the left.
    pub frozen_cols: u32,
    /// Whether gridlines are painted.
    pub show_gridlines: bool,
    /// Default row height, CSS pixels at 100% zoom.
    pub default_row_height: u32,
    /// Default column width, CSS pixels at 100% zoom.
    pub default_col_width: u32,
}

impl Codec for SheetInfo {
    fn write(&self, w: &mut Writer) {
        w.u32v(self.id);
        w.str(&self.name);
        w.u8(self.visibility.to_u8());
        w.u32v(self.frozen_rows);
        w.u32v(self.frozen_cols);
        w.bool(self.show_gridlines);
        w.u32v(self.default_row_height);
        w.u32v(self.default_col_width);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            id: r.u32v()?,
            name: r.str()?,
            visibility: Visibility::from_u8(r.u8()?)?,
            frozen_rows: r.u32v()?,
            frozen_cols: r.u32v()?,
            show_gridlines: r.bool()?,
            default_row_height: r.u32v()?,
            default_col_width: r.u32v()?,
        })
    }
}

wire_enum! {
    /// Which axis of a sheet.
    pub enum Axis ("axis") {
        /// Row heights.
        Rows = 0,
        /// Column widths.
        Cols = 1,
    }
}

/// One override on an axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisEntry {
    /// Row or column index.
    pub index: u32,
    /// Size in CSS pixels at 100% zoom; 0 is hidden.
    pub size: u32,
}

impl Codec for AxisEntry {
    fn write(&self, w: &mut Writer) {
        w.u32v(self.index);
        w.u32v(self.size);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            index: r.u32v()?,
            size: r.u32v()?,
        })
    }
}

/// Row heights or column widths that differ from the sheet's default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxisChange {
    /// Sheet id.
    pub sheet: u32,
    /// Rows or columns.
    pub axis: Axis,
    /// `true`: `entries` is the axis's complete override set. `false`: each entry sets one index,
    /// and an entry at the default size removes that override.
    pub replace: bool,
    /// Overrides, indexes ascending and unique.
    pub entries: Vec<AxisEntry>,
}

impl Codec for AxisChange {
    fn write(&self, w: &mut Writer) {
        w.u32v(self.sheet);
        w.u8(self.axis.to_u8());
        w.bool(self.replace);
        write_list(w, &self.entries);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            sheet: r.u32v()?,
            axis: Axis::from_u8(r.u8()?)?,
            replace: r.bool()?,
            entries: read_list(r, "axis entries", MAX_AXIS_ENTRIES)?,
        })
    }
}

wire_enum! {
    /// Underline style.
    pub enum Underline ("underline") {
        /// None.
        None = 0,
        /// Single.
        Single = 1,
        /// Double.
        Double = 2,
        /// Single accounting (spans the cell width).
        SingleAccounting = 3,
        /// Double accounting.
        DoubleAccounting = 4,
    }
}

wire_enum! {
    /// Fill pattern, in OOXML `ST_PatternType` order.
    pub enum FillPattern ("fill pattern") {
        /// No fill.
        None = 0,
        /// Solid foreground color.
        Solid = 1,
        /// 50% gray.
        MediumGray = 2,
        /// 75% gray.
        DarkGray = 3,
        /// 25% gray.
        LightGray = 4,
        /// Thick horizontal stripes.
        DarkHorizontal = 5,
        /// Thick vertical stripes.
        DarkVertical = 6,
        /// Thick reverse diagonal stripes.
        DarkDown = 7,
        /// Thick diagonal stripes.
        DarkUp = 8,
        /// Thick diagonal crosshatch.
        DarkGrid = 9,
        /// Thick trellis.
        DarkTrellis = 10,
        /// Thin horizontal stripes.
        LightHorizontal = 11,
        /// Thin vertical stripes.
        LightVertical = 12,
        /// Thin reverse diagonal stripes.
        LightDown = 13,
        /// Thin diagonal stripes.
        LightUp = 14,
        /// Thin horizontal crosshatch.
        LightGrid = 15,
        /// Thin diagonal crosshatch.
        LightTrellis = 16,
        /// 12.5% gray.
        Gray125 = 17,
        /// 6.25% gray.
        Gray0625 = 18,
    }
}

wire_enum! {
    /// Horizontal alignment.
    pub enum HAlign ("horizontal alignment") {
        /// General: numbers right, text left, booleans and errors centered.
        General = 0,
        /// Left.
        Left = 1,
        /// Center.
        Center = 2,
        /// Right.
        Right = 3,
        /// Repeat the text to fill the cell.
        Fill = 4,
        /// Justify.
        Justify = 5,
        /// Center across selection.
        CenterContinuous = 6,
        /// Distributed.
        Distributed = 7,
    }
}

wire_enum! {
    /// Vertical alignment.
    pub enum VAlign ("vertical alignment") {
        /// Bottom.
        Bottom = 0,
        /// Center.
        Center = 1,
        /// Top.
        Top = 2,
        /// Justify.
        Justify = 3,
        /// Distributed.
        Distributed = 4,
    }
}

wire_enum! {
    /// Border line style, in OOXML `ST_BorderStyle` order.
    pub enum BorderStyle ("border style") {
        /// No border.
        None = 0,
        /// Thin.
        Thin = 1,
        /// Medium.
        Medium = 2,
        /// Dashed.
        Dashed = 3,
        /// Dotted.
        Dotted = 4,
        /// Thick.
        Thick = 5,
        /// Double.
        Double = 6,
        /// Hair.
        Hair = 7,
        /// Medium dashed.
        MediumDashed = 8,
        /// Dash dot.
        DashDot = 9,
        /// Medium dash dot.
        MediumDashDot = 10,
        /// Dash dot dot.
        DashDotDot = 11,
        /// Medium dash dot dot.
        MediumDashDotDot = 12,
        /// Slanted dash dot.
        SlantDashDot = 13,
    }
}

/// One border line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Border {
    /// Line style.
    pub style: BorderStyle,
    /// Line color.
    pub color: Rgba,
}

impl Codec for Border {
    fn write(&self, w: &mut Writer) {
        w.u8(self.style.to_u8());
        self.color.write(w);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            style: BorderStyle::from_u8(r.u8()?)?,
            color: Rgba::read(r)?,
        })
    }
}

/// `Style::font_flags` bits.
pub mod font_flags {
    /// Bold.
    pub const BOLD: u8 = 0x01;
    /// Italic.
    pub const ITALIC: u8 = 0x02;
    /// Strikethrough.
    pub const STRIKE: u8 = 0x04;
    /// Superscript.
    pub const SUPERSCRIPT: u8 = 0x08;
    /// Subscript.
    pub const SUBSCRIPT: u8 = 0x10;
    /// Every known bit.
    pub const ALL: u8 = 0x1f;
}

/// `Style::text_flags` bits.
pub mod text_flags {
    /// Wrap text.
    pub const WRAP: u8 = 0x01;
    /// Shrink to fit.
    pub const SHRINK: u8 = 0x02;
    /// Every known bit.
    pub const ALL: u8 = 0x03;
}

/// `Style::diagonal` bits.
pub mod diagonal {
    /// Bottom-left to top-right.
    pub const UP: u8 = 0x01;
    /// Top-left to bottom-right.
    pub const DOWN: u8 = 0x02;
    /// Every known bit.
    pub const ALL: u8 = 0x03;
}

/// An effective cell style: the cell's formatting with conditional formatting applied.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Style {
    /// Font family.
    pub font_name: String,
    /// Font size in hundredths of a point.
    pub font_size: u32,
    /// [`font_flags`] bits.
    pub font_flags: u8,
    /// Underline.
    pub underline: Underline,
    /// Font color.
    pub font_color: Rgba,
    /// Fill pattern.
    pub fill_pattern: FillPattern,
    /// Pattern foreground (the color of a solid fill).
    pub fill_fg: Rgba,
    /// Pattern background.
    pub fill_bg: Rgba,
    /// Horizontal alignment.
    pub h_align: HAlign,
    /// Vertical alignment.
    pub v_align: VAlign,
    /// [`text_flags`] bits.
    pub text_flags: u8,
    /// Indent level.
    pub indent: u32,
    /// OOXML `textRotation`: 0-90 counterclockwise, 91-180 clockwise (90 + degrees), 255 stacked.
    pub rotation: u8,
    /// Left border.
    pub border_left: Border,
    /// Right border.
    pub border_right: Border,
    /// Top border.
    pub border_top: Border,
    /// Bottom border.
    pub border_bottom: Border,
    /// Diagonal border line, drawn in the directions given by `diagonal`.
    pub border_diagonal: Border,
    /// [`diagonal`] bits.
    pub diagonal: u8,
    /// Number format code; `"General"` for General.
    pub number_format: String,
}

impl Codec for Style {
    fn write(&self, w: &mut Writer) {
        w.str(&self.font_name);
        w.u32v(self.font_size);
        w.u8(self.font_flags);
        w.u8(self.underline.to_u8());
        self.font_color.write(w);
        w.u8(self.fill_pattern.to_u8());
        self.fill_fg.write(w);
        self.fill_bg.write(w);
        w.u8(self.h_align.to_u8());
        w.u8(self.v_align.to_u8());
        w.u8(self.text_flags);
        w.u32v(self.indent);
        w.u8(self.rotation);
        for border in [
            &self.border_left,
            &self.border_right,
            &self.border_top,
            &self.border_bottom,
            &self.border_diagonal,
        ] {
            border.write(w);
        }
        w.u8(self.diagonal);
        w.str(&self.number_format);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            font_name: r.str()?,
            font_size: r.u32v()?,
            font_flags: read_flags(r, "font flags", font_flags::ALL)?,
            underline: Underline::from_u8(r.u8()?)?,
            font_color: Rgba::read(r)?,
            fill_pattern: FillPattern::from_u8(r.u8()?)?,
            fill_fg: Rgba::read(r)?,
            fill_bg: Rgba::read(r)?,
            h_align: HAlign::from_u8(r.u8()?)?,
            v_align: VAlign::from_u8(r.u8()?)?,
            text_flags: read_flags(r, "text flags", text_flags::ALL)?,
            indent: r.u32v()?,
            rotation: match r.u8()? {
                v @ (0..=180 | 255) => v,
                value => {
                    return Err(DecodeError::BadTag {
                        what: "rotation",
                        value,
                    })
                }
            },
            border_left: Border::read(r)?,
            border_right: Border::read(r)?,
            border_top: Border::read(r)?,
            border_bottom: Border::read(r)?,
            border_diagonal: Border::read(r)?,
            diagonal: read_flags(r, "diagonal", diagonal::ALL)?,
            number_format: r.str()?,
        })
    }
}

/// [`CellData::flags`] bits.
pub mod cell_flags {
    /// The cell has a note (red corner marker).
    pub const NOTE: u8 = 0x01;
    /// The cell has a threaded comment.
    pub const COMMENT: u8 = 0x02;
    /// The cell has list validation (dropdown arrow when active).
    pub const LIST_VALIDATION: u8 = 0x04;
    /// Error-checking indicator (green triangle).
    pub const ERROR_INDICATOR: u8 = 0x08;
    /// The cell is the anchor of a dynamic-array spill.
    pub const SPILL_ANCHOR: u8 = 0x10;
    /// The cell is inside a spill range.
    pub const SPILLED: u8 = 0x20;
    /// On the wire only: a decoration follows. Set and cleared by the codec from
    /// [`CellData::decoration`]; ignored in [`CellData::flags`].
    pub const DECORATED: u8 = 0x40;
    /// Paint the decoration only, not the text (CF "show bar only" / "show icon only").
    pub const HIDE_TEXT: u8 = 0x80;
}

/// What a cell holds and shows. Text is the display text at unlimited column width.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    /// No value; present because of its style, flags or decoration.
    Blank,
    /// A number and its display text.
    Number {
        /// The value, for width-dependent formatting on the client.
        value: f64,
        /// Display text.
        text: String,
    },
    /// Text.
    Text(String),
    /// A boolean's display text.
    Bool(String),
    /// An error's display text (`#DIV/0!`).
    Error(String),
}

/// A data bar painted in the cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DataBar {
    /// Where the bar starts, as a fraction of the cell's inner width.
    pub start: f32,
    /// Where the bar ends, as a fraction of the cell's inner width.
    pub end: f32,
    /// Bar fill (the negative color already chosen for negative values).
    pub fill: Rgba,
    /// Bar border, if any.
    pub border: Option<Rgba>,
    /// Gradient rather than solid fill.
    pub gradient: bool,
    /// Where the axis is drawn, as a fraction of the cell's inner width, if it is drawn.
    pub axis: Option<f32>,
}

const BAR_GRADIENT: u8 = 0x01;
const BAR_BORDER: u8 = 0x02;
const BAR_AXIS: u8 = 0x04;

impl Codec for DataBar {
    fn write(&self, w: &mut Writer) {
        w.f32(self.start);
        w.f32(self.end);
        self.fill.write(w);
        self.border.unwrap_or(Rgba([0; 4])).write(w);
        let mut flags = 0;
        if self.gradient {
            flags |= BAR_GRADIENT;
        }
        if self.border.is_some() {
            flags |= BAR_BORDER;
        }
        if self.axis.is_some() {
            flags |= BAR_AXIS;
        }
        w.u8(flags);
        if let Some(axis) = self.axis {
            w.f32(axis);
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let start = r.f32()?;
        let end = r.f32()?;
        let fill = Rgba::read(r)?;
        let border = Rgba::read(r)?;
        let flags = read_flags(r, "data bar flags", BAR_GRADIENT | BAR_BORDER | BAR_AXIS)?;
        Ok(Self {
            start,
            end,
            fill,
            border: (flags & BAR_BORDER != 0).then_some(border),
            gradient: flags & BAR_GRADIENT != 0,
            axis: if flags & BAR_AXIS == 0 {
                None
            } else {
                Some(r.f32()?)
            },
        })
    }
}

/// An icon-set icon painted in the cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Icon {
    /// Icon set, in OOXML `ST_IconSetType` order.
    pub set: u8,
    /// Icon within the set.
    pub index: u8,
}

/// Conditional-formatting graphics that are not part of a style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decoration {
    /// Data bar.
    pub data_bar: Option<DataBar>,
    /// Icon.
    pub icon: Option<Icon>,
}

const PART_BAR: u8 = 0x01;
const PART_ICON: u8 = 0x02;

impl Decoration {
    fn is_empty(&self) -> bool {
        self.data_bar.is_none() && self.icon.is_none()
    }
}

impl Codec for Decoration {
    fn write(&self, w: &mut Writer) {
        let mut parts = 0;
        if self.data_bar.is_some() {
            parts |= PART_BAR;
        }
        if self.icon.is_some() {
            parts |= PART_ICON;
        }
        w.u8(parts);
        if let Some(bar) = &self.data_bar {
            bar.write(w);
        }
        if let Some(icon) = self.icon {
            w.u8(icon.set);
            w.u8(icon.index);
        }
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        let parts = r.u8()?;
        if parts == 0 || parts & !(PART_BAR | PART_ICON) != 0 {
            return Err(DecodeError::BadTag {
                what: "decoration parts",
                value: parts,
            });
        }
        let data_bar = if parts & PART_BAR == 0 {
            None
        } else {
            Some(DataBar::read(r)?)
        };
        let icon = if parts & PART_ICON == 0 {
            None
        } else {
            Some(Icon {
                set: r.u8()?,
                index: r.u8()?,
            })
        };
        Ok(Self { data_bar, icon })
    }
}

/// A non-default cell.
#[derive(Clone, Debug, PartialEq)]
pub struct CellData {
    /// Value and display text.
    pub value: CellValue,
    /// [`cell_flags`] bits (`DECORATED` is managed by the codec).
    pub flags: u8,
    /// Effective style id.
    pub style: u32,
    /// Data bar and icon, if any. A decoration with neither part is sent as none.
    pub decoration: Option<Decoration>,
}

/// One record of a tile's or patch's cell list.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// `row · tile_cols + col`, relative to the tile.
    pub index: u32,
    /// The cell, or `None` in a patch when the cell became default.
    pub data: Option<CellData>,
}

const KIND_BLANK: u8 = 0;
const KIND_NUMBER: u8 = 1;
const KIND_TEXT: u8 = 2;
const KIND_BOOL: u8 = 3;
const KIND_ERROR: u8 = 4;
const KIND_CLEARED: u8 = 5;

/// Writes a cell list, delta-encoding indexes. Cells must be sorted by index, without repeats.
pub(crate) fn write_cells(w: &mut Writer, cells: &[Cell]) {
    w.len(cells.len());
    let mut prev: i64 = -1;
    for cell in cells {
        let index = i64::from(cell.index);
        debug_assert!(
            index > prev,
            "cells must be sorted by index without repeats"
        );
        // Unsorted input is a caller bug; encode a zero delta rather than wrap around, and let
        // the receiver's ordering check reject the message.
        w.uv(u64::try_from(index - prev - 1).unwrap_or(0));
        prev = index;
        let Some(data) = &cell.data else {
            w.u8(KIND_CLEARED);
            continue;
        };
        let decoration = data.decoration.filter(|d| !d.is_empty());
        let kind = match &data.value {
            CellValue::Blank => KIND_BLANK,
            CellValue::Number { .. } => KIND_NUMBER,
            CellValue::Text(_) => KIND_TEXT,
            CellValue::Bool(_) => KIND_BOOL,
            CellValue::Error(_) => KIND_ERROR,
        };
        w.u8(kind);
        let mut flags = data.flags & !cell_flags::DECORATED;
        if decoration.is_some() {
            flags |= cell_flags::DECORATED;
        }
        w.u8(flags);
        w.u32v(data.style);
        match &data.value {
            CellValue::Blank => {}
            CellValue::Number { value, text } => {
                w.str(text);
                w.f64(*value);
            }
            CellValue::Text(text) | CellValue::Bool(text) | CellValue::Error(text) => w.str(text),
        }
        if let Some(decoration) = decoration {
            decoration.write(w);
        }
    }
}

/// Reads a cell list. Indexes come out strictly ascending and below [`MAX_TILE_CELLS`].
pub(crate) fn read_cells(r: &mut Reader<'_>) -> Result<Vec<Cell>, DecodeError> {
    let n = r.count("cells", MAX_TILE_CELLS)?;
    let mut cells = Vec::with_capacity(n);
    let mut next: u64 = 0;
    for _ in 0..n {
        let index = next
            .checked_add(r.uv()?)
            .filter(|&i| i < MAX_TILE_CELLS as u64)
            .ok_or(DecodeError::BadInteger)?;
        next = index + 1;
        // Bounded by MAX_TILE_CELLS just above.
        #[allow(clippy::cast_possible_truncation)]
        let index = index as u32;
        let kind = r.u8()?;
        if kind == KIND_CLEARED {
            cells.push(Cell { index, data: None });
            continue;
        }
        let flags = r.u8()?;
        let style = r.u32v()?;
        let value = match kind {
            KIND_BLANK => CellValue::Blank,
            KIND_NUMBER => {
                let text = r.str()?;
                CellValue::Number {
                    value: r.f64()?,
                    text,
                }
            }
            KIND_TEXT => CellValue::Text(r.str()?),
            KIND_BOOL => CellValue::Bool(r.str()?),
            KIND_ERROR => CellValue::Error(r.str()?),
            value => {
                return Err(DecodeError::BadTag {
                    what: "cell kind",
                    value,
                })
            }
        };
        let decoration = if flags & cell_flags::DECORATED == 0 {
            None
        } else {
            Some(Decoration::read(r)?)
        };
        cells.push(Cell {
            index,
            data: Some(CellData {
                value,
                flags: flags & !cell_flags::DECORATED,
                style,
                decoration,
            }),
        });
    }
    Ok(cells)
}

/// A tile's complete contents.
#[derive(Clone, Debug, PartialEq)]
pub struct TileBody {
    /// Sheet id.
    pub sheet: u32,
    /// Tile row in the lattice (covers rows `tile_row · tile_rows ..`).
    pub tile_row: u32,
    /// Tile column in the lattice.
    pub tile_col: u32,
    /// Merged ranges intersecting the tile, in sheet coordinates.
    pub merges: Vec<Rect>,
    /// Non-default cells, sorted by index.
    pub cells: Vec<Cell>,
}

impl Codec for TileBody {
    fn write(&self, w: &mut Writer) {
        w.u32v(self.sheet);
        w.u32v(self.tile_row);
        w.u32v(self.tile_col);
        write_list(w, &self.merges);
        write_cells(w, &self.cells);
    }

    fn read(r: &mut Reader<'_>) -> Result<Self, DecodeError> {
        Ok(Self {
            sheet: r.u32v()?,
            tile_row: r.u32v()?,
            tile_col: r.u32v()?,
            merges: read_list(r, "merges", MAX_MERGES)?,
            cells: read_cells(r)?,
        })
    }
}
