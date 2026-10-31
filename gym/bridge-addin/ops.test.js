import { describe, expect, it } from "vitest";
import { dispatch, makeOps } from "./ops.js";

/** A minimal fake of the Office.js surface the ops touch. */
function fakeExcel() {
  const writes = [];
  const range = (address) => ({
    address,
    get values() {
      return [[1]];
    },
    set values(v) {
      writes.push(["values", address, v]);
    },
    load() {},
    select() {
      writes.push(["select", address]);
    },
    text: [["1"]],
    formulas: [["=1"]],
    numberFormat: [["General"]],
    valueTypes: [["Double"]],
  });
  const sheet = { getRange: range };
  const ctx = {
    workbook: { worksheets: { getActiveWorksheet: () => sheet } },
    sync: async () => {},
  };
  return { Excel: { run: (fn) => fn(ctx) }, writes };
}

describe("bridge dispatch", () => {
  it("runs an op and wraps its result", async () => {
    const { Excel, writes } = fakeExcel();
    const reply = await dispatch(makeOps(Excel), { id: 7, op: "setValues", args: { address: "A1:B1", values: [[1, 2]] } });
    expect(reply).toEqual({ id: 7, ok: true, result: {} });
    expect(writes).toEqual([["values", "A1:B1", [[1, 2]]]]);
  });

  it("reads back what Excel holds", async () => {
    const { Excel } = fakeExcel();
    const reply = await dispatch(makeOps(Excel), { id: 1, op: "read", args: { address: "A1" } });
    expect(reply.ok).toBe(true);
    expect(reply.result.text).toEqual([["1"]]);
  });

  it("turns unknown ops and thrown errors into error replies", async () => {
    const { Excel } = fakeExcel();
    const ops = makeOps(Excel);
    expect(await dispatch(ops, { id: 2, op: "nope" })).toMatchObject({ id: 2, ok: false, error: { message: "unknown op nope" } });
    expect(await dispatch(ops, { id: 3, op: "toString" })).toMatchObject({ ok: false });
    const reply = await dispatch(ops, { id: 4, op: "eval", args: { code: "throw new Error('boom')" } });
    expect(reply).toMatchObject({ id: 4, ok: false, error: { message: "boom" } });
  });

  it("evaluates arbitrary Office.js code with the context", async () => {
    const { Excel } = fakeExcel();
    const reply = await dispatch(makeOps(Excel), {
      id: 5,
      op: "eval",
      args: { code: "const r = context.workbook.worksheets.getActiveWorksheet().getRange('C3'); return r.address;" },
    });
    expect(reply).toEqual({ id: 5, ok: true, result: "C3" });
  });
});
