//! rustsheet's number formatter, exported to JavaScript.
//!
//! The grid needs Excel's display text for cells whose rendering depends on the column's pixel
//! width (General's shortening, `#####`). The server cannot know the width, so the browser runs
//! the engine's own formatter: one implementation, no drift between server and client.
//!
//! This first slice exports formatting at a fixed format code; the width-aware General display
//! comes with the engine API task that adds it.
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use rustsheet_engine::formatter::format::{format_number, FormatContext};
use rustsheet_engine::locale::get_default_locale;
use wasm_bindgen::prelude::wasm_bindgen;

/// `value` formatted with the Excel format code `format` (en-US, 1900 date system).
///
/// Returns the display text; a value the format cannot show (a negative date) displays as `#`
/// characters, as Excel shows it in a cell.
#[wasm_bindgen(js_name = formatNumber)]
#[must_use]
pub fn format_number_js(value: f64, format: &str) -> String {
    let ctx = FormatContext::new(get_default_locale());
    let formatted = format_number(value, format, &ctx);
    if formatted.unshowable {
        "#".repeat(255)
    } else {
        formatted.text
    }
}

#[cfg(test)]
mod tests {
    use super::format_number_js;

    #[test]
    fn formats_with_excel_codes() {
        assert_eq!(format_number_js(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(format_number_js(0.25, "0%"), "25%");
        assert_eq!(format_number_js(45000.0, "yyyy-mm-dd"), "2023-03-15");
    }
}
