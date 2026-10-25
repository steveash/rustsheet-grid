// Bridge operations: each runs inside Excel.run and returns plain JSON.
// Kept free of DOM and WebSocket code so the dispatcher can be unit-tested with a fake Excel.

/** @param {any} Excel the Office.js Excel namespace */
export function makeOps(Excel) {
  const run = (fn) => Excel.run(fn);
  const sheetOf = (ctx) => ctx.workbook.worksheets.getActiveWorksheet();

  return {
    async ping() {
      return { ok: true, at: Date.now() };
    },

    // A clean slate: a fresh sheet named `name`, every other sheet deleted, A1 selected (which
    // scrolls the view to the top-left, the state the gym's geometry assumes).
    async reset({ name = "Gym" } = {}) {
      return run(async (ctx) => {
        const sheets = ctx.workbook.worksheets;
        sheets.load("items/name");
        await ctx.sync();
        const temp = sheets.add(`gym-${Date.now()}`);
        temp.activate();
        await ctx.sync();
        for (const s of sheets.items) s.delete();
        await ctx.sync();
        temp.name = name;
        temp.getRange("A1").select();
        await ctx.sync();
        return { sheet: name };
      });
    },

    // Replaces the workbook's sheets with those of an .xlsx (base64), then selects A1.
    async loadXlsx({ base64 }) {
      return run(async (ctx) => {
        const sheets = ctx.workbook.worksheets;
        sheets.load("items/name");
        await ctx.sync();
        // Rename the old sheets first so the fixture's names ("Sheet1") cannot collide with them.
        const stamp = Date.now();
        const old = sheets.items.map((s, i) => {
          s.name = `gym-old-${stamp}-${i}`;
          return s.name;
        });
        await ctx.sync();
        ctx.workbook.insertWorksheetsFromBase64(base64);
        await ctx.sync();
        for (const name of old) sheets.getItem(name).delete();
        const first = sheets.getFirst();
        first.activate();
        first.getRange("A1").select();
        first.load("name");
        await ctx.sync();
        return { sheet: first.name };
      });
    },

    async setValues({ address, values }) {
      return run(async (ctx) => {
        sheetOf(ctx).getRange(address).values = values;
        await ctx.sync();
        return {};
      });
    },

    async setFormulas({ address, formulas }) {
      return run(async (ctx) => {
        sheetOf(ctx).getRange(address).formulas = formulas;
        await ctx.sync();
        return {};
      });
    },

    async setNumberFormat({ address, numberFormat }) {
      return run(async (ctx) => {
        sheetOf(ctx).getRange(address).numberFormat = numberFormat;
        await ctx.sync();
        return {};
      });
    },

    // `format` is an Excel.RangeFormat property bag (fill, font, alignment, wrapText, ...);
    // `borders` is a list of { side, style, color, weight }.
    async setFormat({ address, format = {}, borders = [] }) {
      return run(async (ctx) => {
        const range = sheetOf(ctx).getRange(address);
        range.format.set(format);
        for (const { side, ...props } of borders) range.format.borders.getItem(side).set(props);
        await ctx.sync();
        return {};
      });
    },

    // `type` is an Excel.ConditionalFormatType ("DataBar", "ColorScale", "IconSet", "CellValue",
    // "ContainsText", "TopBottom", "PresetCriteria", "Custom"); `props` is set on the matching
    // sub-object (cf.dataBar, cf.colorScale, ...); `stopIfTrue`/`priority` on the format itself.
    async addConditionalFormat({ address, type, props = {}, stopIfTrue, priority }) {
      return run(async (ctx) => {
        const cf = sheetOf(ctx).getRange(address).conditionalFormats.add(type);
        const sub = cf[type.charAt(0).toLowerCase() + type.slice(1)];
        if (!sub) throw new Error(`unknown conditional format type ${type}`);
        sub.set(props);
        if (stopIfTrue !== undefined) cf.stopIfTrue = stopIfTrue;
        if (priority !== undefined) cf.priority = priority;
        await ctx.sync();
        return {};
      });
    },

    // Width in points for columns (e.g. "B:B"), height in points for rows (e.g. "3:3").
    async setSize({ address, columnWidth, rowHeight }) {
      return run(async (ctx) => {
        const range = sheetOf(ctx).getRange(address);
        if (columnWidth !== undefined) range.format.columnWidth = columnWidth;
        if (rowHeight !== undefined) range.format.rowHeight = rowHeight;
        await ctx.sync();
        return {};
      });
    },

    async freeze({ rows = 0, columns = 0 }) {
      return run(async (ctx) => {
        const panes = sheetOf(ctx).freezePanes;
        panes.unfreeze();
        if (rows > 0 && columns > 0) panes.freezeAt(sheetOf(ctx).getRangeByIndexes(0, 0, rows, columns));
        else if (rows > 0) panes.freezeRows(rows);
        else if (columns > 0) panes.freezeColumns(columns);
        await ctx.sync();
        return {};
      });
    },

    async select({ address }) {
      return run(async (ctx) => {
        sheetOf(ctx).getRange(address).select();
        await ctx.sync();
        return {};
      });
    },

    // What Excel holds for a range: values, display text, formulas, number formats.
    async read({ address }) {
      return run(async (ctx) => {
        const range = sheetOf(ctx).getRange(address);
        range.load("address,values,text,formulas,numberFormat,valueTypes");
        await ctx.sync();
        return {
          address: range.address,
          values: range.values,
          text: range.text,
          formulas: range.formulas,
          numberFormat: range.numberFormat,
          valueTypes: range.valueTypes,
        };
      });
    },

    // Position and size in points, relative to the sheet's top-left corner, for the range and
    // for each of its columns and rows.
    async geometry({ address }) {
      return run(async (ctx) => {
        const range = sheetOf(ctx).getRange(address);
        range.load("left,top,width,height,columnCount,rowCount");
        await ctx.sync();
        const cols = [];
        const rows = [];
        for (let c = 0; c < range.columnCount; c++) {
          const col = range.getColumn(c);
          col.load("left,width");
          cols.push(col);
        }
        for (let r = 0; r < range.rowCount; r++) {
          const row = range.getRow(r);
          row.load("top,height");
          rows.push(row);
        }
        await ctx.sync();
        return {
          left: range.left,
          top: range.top,
          width: range.width,
          height: range.height,
          columns: cols.map((c) => ({ left: c.left, width: c.width })),
          rows: rows.map((r) => ({ top: r.top, height: r.height })),
        };
      });
    },

    async selection() {
      return run(async (ctx) => {
        const selected = ctx.workbook.getSelectedRange();
        const active = ctx.workbook.getActiveCell();
        selected.load("address");
        active.load("address");
        await ctx.sync();
        return { selected: selected.address, active: active.address };
      });
    },

    // Escape hatch so probes can use any Office.js API without a gym redeploy. `code` is the body
    // of an async function receiving (Excel, context); its return value must be JSON.
    async eval({ code }) {
      const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;
      const fn = new AsyncFunction("Excel", "context", code);
      return run((ctx) => fn(Excel, ctx));
    },
  };
}

/**
 * Runs one request `{ id, op, args }` and returns the reply `{ id, ok, result | error }`.
 * Never throws: every failure becomes an error reply.
 */
export async function dispatch(ops, request) {
  const { id, op, args } = request ?? {};
  try {
    const fn = Object.hasOwn(ops, op) ? ops[op] : undefined;
    if (!fn) throw new Error(`unknown op ${op}`);
    const result = await fn(args ?? {});
    return { id, ok: true, result: result ?? {} };
  } catch (e) {
    const debug = e && typeof e === "object" && "debugInfo" in e ? e.debugInfo : undefined;
    return { id, ok: false, error: { message: String(e?.message ?? e), code: e?.code, debug } };
  }
}
