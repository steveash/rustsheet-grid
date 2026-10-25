//! Rendering one rectangular tile of a sheet.

use rustsheet::{CellRef, RefError, SheetId, Workbook};

/// Most cells one tile may hold. Tiles are requested by the client, so the size is untrusted.
pub const MAX_TILE_CELLS: u32 = 64 * 64;

/// A rectangle of cells on one sheet, 0-based, in rows × columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRect {
    /// Worksheet.
    pub sheet: SheetId,
    /// First row.
    pub top: u32,
    /// First column.
    pub left: u32,
    /// Number of rows.
    pub rows: u32,
    /// Number of columns.
    pub cols: u32,
}

/// The display text of every cell in a [`TileRect`], row-major. Empty cells are empty strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    /// The rectangle this tile covers.
    pub rect: TileRect,
    /// `rows × cols` display strings, row-major.
    pub text: Vec<String>,
}

/// Why a tile could not be rendered.
#[derive(Debug)]
pub enum TileError {
    /// The tile is empty or holds more than [`MAX_TILE_CELLS`] cells.
    BadSize,
    /// The rectangle does not resolve in the workbook (unknown sheet, out of the grid).
    Ref(RefError),
}

impl std::fmt::Display for TileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadSize => write!(f, "tile must hold 1 to {MAX_TILE_CELLS} cells"),
            Self::Ref(e) => write!(f, "tile does not resolve: {e:?}"),
        }
    }
}

impl std::error::Error for TileError {}

/// Renders the display text of `rect`, as Excel shows each cell at its stored number format.
///
/// Column-width effects (General's shortening, `#####`) are applied by the client, which knows
/// the pixel width; this returns the full-width text.
pub fn render_tile(workbook: &Workbook, rect: TileRect) -> Result<Tile, TileError> {
    let cells = rect.rows.checked_mul(rect.cols).ok_or(TileError::BadSize)?;
    if cells == 0 || cells > MAX_TILE_CELLS {
        return Err(TileError::BadSize);
    }
    let mut text = Vec::with_capacity(cells as usize);
    for row in rect.top..rect.top.saturating_add(rect.rows) {
        for col in rect.left..rect.left.saturating_add(rect.cols) {
            let cell = CellRef {
                sheet: rect.sheet,
                row,
                col,
            };
            text.push(workbook.formatted(cell).map_err(TileError::Ref)?);
        }
    }
    Ok(Tile { rect, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustsheet::WorkbookOptions;

    fn workbook() -> (Workbook, SheetId) {
        let wb = Workbook::new(&WorkbookOptions::default()).unwrap();
        let sheet = wb.sheets()[0].id;
        (wb, sheet)
    }

    #[test]
    fn renders_values_and_formulas_row_major() {
        let (mut wb, sheet) = workbook();
        wb.edit(|tx| {
            tx.set("A1", "1.5")?;
            tx.set("B1", "=A1*2")?;
            tx.set("A2", "hello")
        })
        .unwrap();
        let rect = TileRect {
            sheet,
            top: 0,
            left: 0,
            rows: 2,
            cols: 2,
        };
        let tile = render_tile(&wb, rect).unwrap();
        assert_eq!(tile.text, vec!["1.5", "3", "hello", ""]);
    }

    #[test]
    fn refuses_empty_and_oversized_tiles() {
        let (wb, sheet) = workbook();
        let mut rect = TileRect {
            sheet,
            top: 0,
            left: 0,
            rows: 0,
            cols: 4,
        };
        assert!(matches!(render_tile(&wb, rect), Err(TileError::BadSize)));
        rect.rows = 65;
        rect.cols = 64;
        assert!(matches!(render_tile(&wb, rect), Err(TileError::BadSize)));
        rect.rows = u32::MAX;
        rect.cols = u32::MAX;
        assert!(matches!(render_tile(&wb, rect), Err(TileError::BadSize)));
    }
}
