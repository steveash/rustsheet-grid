//! Codec tests: every message round-trips, the bytes of one message are pinned, and malformed
//! input fails cleanly.

use crate::*;

fn rect(top: u32, left: u32, rows: u32, cols: u32) -> Rect {
    Rect {
        top,
        left,
        rows,
        cols,
    }
}

fn style() -> Style {
    let none = Border {
        style: BorderStyle::None,
        color: Rgba([0, 0, 0, 255]),
    };
    Style {
        font_name: "Aptos Narrow".into(),
        font_size: 1100,
        font_flags: font_flags::BOLD | font_flags::ITALIC,
        underline: Underline::DoubleAccounting,
        font_color: Rgba([0x1f, 0x2e, 0x3d, 255]),
        fill_pattern: FillPattern::Gray0625,
        fill_fg: Rgba([255, 255, 0, 255]),
        fill_bg: Rgba([1, 2, 3, 4]),
        h_align: HAlign::Distributed,
        v_align: VAlign::Top,
        text_flags: text_flags::WRAP,
        indent: 3,
        rotation: 255,
        border_left: Border {
            style: BorderStyle::SlantDashDot,
            color: Rgba([9, 8, 7, 255]),
        },
        border_right: none,
        border_top: none,
        border_bottom: Border {
            style: BorderStyle::Double,
            color: Rgba([0, 0, 0, 255]),
        },
        border_diagonal: none,
        diagonal: diagonal::UP,
        number_format: "#,##0.00;[Red]-#,##0.00".into(),
    }
}

fn sheet() -> SheetInfo {
    SheetInfo {
        id: 7,
        name: "Q3 — model".into(),
        visibility: Visibility::VeryHidden,
        frozen_rows: 1,
        frozen_cols: 2,
        show_gridlines: false,
        default_row_height: 20,
        default_col_width: 64,
    }
}

fn cells() -> Vec<Cell> {
    vec![
        Cell {
            index: 0,
            data: Some(CellData {
                value: CellValue::Number {
                    value: -1234.5,
                    text: "-1,234.50".into(),
                },
                flags: cell_flags::NOTE | cell_flags::HIDE_TEXT,
                style: 3,
                decoration: Some(Decoration {
                    data_bar: Some(DataBar {
                        start: 0.25,
                        end: 0.5,
                        fill: Rgba([255, 0, 0, 255]),
                        border: Some(Rgba([200, 0, 0, 255])),
                        gradient: true,
                        axis: Some(0.5),
                    }),
                    icon: Some(Icon { set: 4, index: 2 }),
                }),
            }),
        },
        Cell {
            index: 1,
            data: Some(CellData {
                value: CellValue::Text("héllo".into()),
                flags: 0,
                style: 0,
                decoration: Some(Decoration {
                    data_bar: Some(DataBar {
                        start: 0.0,
                        end: 1.0,
                        fill: Rgba([0, 0, 255, 255]),
                        border: None,
                        gradient: false,
                        axis: None,
                    }),
                    icon: None,
                }),
            }),
        },
        Cell {
            index: 40,
            data: Some(CellData {
                value: CellValue::Bool("TRUE".into()),
                flags: cell_flags::SPILLED,
                style: 70_000,
                decoration: None,
            }),
        },
        Cell {
            index: 41,
            data: Some(CellData {
                value: CellValue::Error("#DIV/0!".into()),
                flags: cell_flags::ERROR_INDICATOR,
                style: 1,
                decoration: None,
            }),
        },
        Cell {
            index: 2000,
            data: Some(CellData {
                value: CellValue::Blank,
                flags: 0,
                style: 9,
                decoration: None,
            }),
        },
        Cell {
            index: 4095,
            data: None,
        },
    ]
}

fn tile() -> TileBody {
    TileBody {
        sheet: 7,
        tile_row: 15_625_000, // row 1,000,000,000
        tile_col: 511,
        merges: vec![rect(1_000_000_000, 16_352, 3, 32)],
        cells: cells(),
    }
}

fn client_messages() -> Vec<ClientMessage> {
    vec![
        ClientMessage::Hello {
            major: MAJOR,
            minor: MINOR,
            client: "grid-core/0.1".into(),
        },
        ClientMessage::Subscribe {
            seq: 42,
            sheet: 7,
            want: vec![rect(0, 0, 1, 2), rect(100, 0, 40, 2)],
            retain: vec![rect(0, 0, 1_048_576, 16_384)],
        },
        ClientMessage::Ack { seq: u64::MAX },
        ClientMessage::Edit {
            req: 1,
            base_revision: 41,
            op: EditOp::SetInput {
                sheet: 7,
                rect: rect(3, 4, 1, 1),
                input: "=SUM(A1:A3)".into(),
            },
        },
        ClientMessage::Edit {
            req: 2,
            base_revision: 42,
            op: EditOp::Clear {
                sheet: 7,
                rect: rect(0, 0, 10, 10),
            },
        },
        ClientMessage::Edit {
            req: 3,
            base_revision: 43,
            op: EditOp::Undo,
        },
        ClientMessage::Edit {
            req: 4,
            base_revision: 44,
            op: EditOp::Redo,
        },
        ClientMessage::Query {
            req: 5,
            query: Query::CellDetail {
                sheet: 7,
                row: 1,
                col: 2,
            },
        },
        ClientMessage::Query {
            req: 6,
            query: Query::JumpEdge {
                sheet: 7,
                row: 1,
                col: 2,
                direction: Direction::Right,
            },
        },
        ClientMessage::Query {
            req: 7,
            query: Query::UsedRange { sheet: 7 },
        },
    ]
}

fn server_messages() -> Vec<ServerMessage> {
    vec![
        ServerMessage::Welcome {
            major: MAJOR,
            minor: MINOR,
            revision: 41,
            tile_rows: TILE_ROWS,
            tile_cols: TILE_COLS,
            max_rows: 1_048_576,
            max_cols: 16_384,
            ack_window: ACK_WINDOW,
        },
        ServerMessage::Sheets {
            revision: 41,
            sheets: vec![sheet(), sheet()],
        },
        ServerMessage::Styles {
            first_id: 0,
            styles: vec![style()],
        },
        ServerMessage::Axis(AxisChange {
            sheet: 7,
            axis: Axis::Rows,
            replace: true,
            entries: vec![
                AxisEntry { index: 3, size: 0 },
                AxisEntry {
                    index: 999_999_999,
                    size: 40,
                },
            ],
        }),
        ServerMessage::SubscribeAck { seq: 42 },
        ServerMessage::Tile {
            revision: 41,
            tile: tile(),
        },
        ServerMessage::Update {
            revision: 42,
            changes: vec![
                Change::Patch {
                    sheet: 7,
                    tile_row: 0,
                    tile_col: 0,
                    cells: cells(),
                },
                Change::Tile(tile()),
                Change::Axis(AxisChange {
                    sheet: 7,
                    axis: Axis::Cols,
                    replace: false,
                    entries: vec![AxisEntry {
                        index: 16_383,
                        size: 64,
                    }],
                }),
                Change::Sheets(vec![sheet()]),
            ],
        },
        ServerMessage::EditResult {
            req: 1,
            revision: 42,
            status: EditStatus::Stale,
            message: "rows were inserted".into(),
        },
        ServerMessage::QueryResult {
            req: 5,
            result: QueryResult::CellDetail {
                input: "=A1*2".into(),
            },
        },
        ServerMessage::QueryResult {
            req: 6,
            result: QueryResult::Cell { row: 9, col: 2 },
        },
        ServerMessage::QueryResult {
            req: 7,
            result: QueryResult::Range(Some(rect(0, 0, 10, 3))),
        },
        ServerMessage::QueryResult {
            req: 8,
            result: QueryResult::Range(None),
        },
        ServerMessage::QueryResult {
            req: 9,
            result: QueryResult::Failed {
                message: "no such sheet".into(),
            },
        },
        ServerMessage::Error {
            code: ErrorCode::Reset,
            message: "style table full".into(),
        },
    ]
}

#[test]
fn client_messages_round_trip() {
    for m in client_messages() {
        assert_eq!(ClientMessage::decode(&m.encode()).as_ref(), Ok(&m), "{m:?}");
    }
}

#[test]
fn server_messages_round_trip() {
    for m in server_messages() {
        assert_eq!(ServerMessage::decode(&m.encode()).as_ref(), Ok(&m), "{m:?}");
    }
}

/// Pins the wire format. If this fails, the change breaks deployed clients: bump `MAJOR`.
#[test]
fn bytes_are_pinned() {
    let hello = ClientMessage::Hello {
        major: 1,
        minor: 0,
        client: "g".into(),
    };
    assert_eq!(hello.encode(), [0x01, 1, 0, 1, b'g']);

    let subscribe = ClientMessage::Subscribe {
        seq: 300,
        sheet: 2,
        want: vec![rect(0, 0, 64, 32)],
        retain: vec![rect(0, 0, 128, 32)],
    };
    assert_eq!(
        subscribe.encode(),
        [0x02, 0xac, 0x02, 2, 1, 0, 0, 64, 32, 1, 0, 0, 0x80, 0x01, 32]
    );

    let tile = ServerMessage::Tile {
        revision: 5,
        tile: TileBody {
            sheet: 0,
            tile_row: 0,
            tile_col: 0,
            merges: vec![],
            cells: vec![
                Cell {
                    index: 2,
                    data: Some(CellData {
                        value: CellValue::Number {
                            value: 1.5,
                            text: "1.5".into(),
                        },
                        flags: 0,
                        style: 0,
                        decoration: None,
                    }),
                },
                Cell {
                    index: 3,
                    data: None,
                },
            ],
        },
    };
    let mut expected = vec![0x86, 5, 0, 0, 0, 0, 2];
    expected.extend([2, 1, 0, 0, 3, b'1', b'.', b'5']);
    expected.extend(1.5f64.to_le_bytes());
    expected.extend([0, 5]);
    assert_eq!(tile.encode(), expected);
}

#[test]
fn empty_decorations_are_not_sent() {
    let mut cell = cells()[2].clone();
    if let Some(data) = cell.data.as_mut() {
        data.decoration = Some(Decoration {
            data_bar: None,
            icon: None,
        });
        // The codec owns this bit.
        data.flags |= cell_flags::DECORATED;
    }
    let message = ServerMessage::Update {
        revision: 1,
        changes: vec![Change::Patch {
            sheet: 0,
            tile_row: 0,
            tile_col: 0,
            cells: vec![cell],
        }],
    };
    let ServerMessage::Update { changes, .. } = ServerMessage::decode(&message.encode()).unwrap()
    else {
        panic!("not an update");
    };
    let Change::Patch { cells, .. } = &changes[0] else {
        panic!("not a patch");
    };
    let data = cells[0].data.as_ref().unwrap();
    assert_eq!(data.decoration, None);
    assert_eq!(data.flags, cell_flags::SPILLED);
}

#[test]
fn every_truncation_fails_cleanly() {
    let all: Vec<Vec<u8>> = client_messages()
        .iter()
        .map(ClientMessage::encode)
        .chain(server_messages().iter().map(ServerMessage::encode))
        .collect();
    for bytes in &all {
        for cut in 0..bytes.len() {
            let prefix = &bytes[..cut];
            assert!(ClientMessage::decode(prefix).is_err() || prefix.is_empty());
            assert!(ServerMessage::decode(prefix).is_err());
        }
    }
}

#[test]
fn trailing_bytes_are_refused() {
    let mut bytes = ClientMessage::Ack { seq: 1 }.encode();
    bytes.push(0);
    assert_eq!(
        ClientMessage::decode(&bytes),
        Err(DecodeError::TrailingBytes)
    );
}

#[test]
fn limits_are_enforced() {
    // Nine `want` rects.
    let too_many = ClientMessage::Subscribe {
        seq: 1,
        sheet: 0,
        want: vec![rect(0, 0, 1, 1); MAX_RECTS + 1],
        retain: vec![rect(0, 0, 1, 1)],
    };
    assert!(matches!(
        ClientMessage::decode(&too_many.encode()),
        Err(DecodeError::TooMany { .. })
    ));
    // No `want` rects.
    let none = ClientMessage::Subscribe {
        seq: 1,
        sheet: 0,
        want: vec![],
        retain: vec![rect(0, 0, 1, 1)],
    };
    assert!(ClientMessage::decode(&none.encode()).is_err());
    // A cell index past the largest tile.
    let outside = ServerMessage::Tile {
        revision: 0,
        tile: TileBody {
            sheet: 0,
            tile_row: 0,
            tile_col: 0,
            merges: vec![],
            cells: vec![Cell {
                index: 4096,
                data: None,
            }],
        },
    };
    assert_eq!(
        ServerMessage::decode(&outside.encode()),
        Err(DecodeError::BadInteger)
    );
}

#[test]
fn unknown_tags_and_bits_are_refused() {
    assert!(matches!(
        ClientMessage::decode(&[0x7f]),
        Err(DecodeError::BadTag { .. })
    ));
    assert!(matches!(
        ServerMessage::decode(&[0x01]),
        Err(DecodeError::BadTag { .. })
    ));
    // A style whose font flags carry an unknown bit.
    let mut bytes = ServerMessage::Styles {
        first_id: 0,
        styles: vec![style()],
    }
    .encode();
    // tag, first_id, count, name (len + 12 bytes), size (2 bytes), then font flags.
    let flags_at = 1 + 1 + 1 + 1 + 12 + 2;
    assert_eq!(bytes[flags_at], font_flags::BOLD | font_flags::ITALIC);
    bytes[flags_at] |= 0x80;
    assert!(matches!(
        ServerMessage::decode(&bytes),
        Err(DecodeError::BadTag {
            what: "font flags",
            ..
        })
    ));
}

/// Random bytes and random mutations of valid messages never panic.
#[test]
fn garbage_never_panics() {
    // xorshift64*: deterministic, no dependency.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_f491_4f6c_dd1d)
    };
    let valid: Vec<Vec<u8>> = client_messages()
        .iter()
        .map(ClientMessage::encode)
        .chain(server_messages().iter().map(ServerMessage::encode))
        .collect();
    for i in 0..20_000 {
        let mut bytes = if i % 2 == 0 {
            let len = usize::try_from(next() % 64).unwrap();
            (0..len)
                .map(|_| next().to_le_bytes()[0])
                .collect::<Vec<u8>>()
        } else {
            valid[usize::try_from(next()).unwrap() % valid.len()].clone()
        };
        if !bytes.is_empty() {
            for _ in 0..=next() % 4 {
                let at = usize::try_from(next()).unwrap() % bytes.len();
                bytes[at] = next().to_le_bytes()[0];
            }
        }
        let _ = ClientMessage::decode(&bytes);
        let _ = ServerMessage::decode(&bytes);
    }
}
