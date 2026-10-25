import { describe, expect, it } from "vitest";
import { AxisGeometry } from "./axis.ts";

describe("AxisGeometry", () => {
  it("uses the default size everywhere without overrides", () => {
    const rows = new AxisGeometry(1_000_000_000, 20);
    expect(rows.total).toBe(20_000_000_000);
    expect(rows.offsetOf(999_999_999)).toBe(19_999_999_980);
    expect(rows.indexAt(19_999_999_985)).toBe(999_999_999);
    expect(rows.indexAt(39)).toBe(1);
  });

  it("shifts offsets after an override and finds indexes on both sides", () => {
    const rows = new AxisGeometry(100, 20);
    rows.setSize(3, 50);
    expect(rows.offsetOf(3)).toBe(60);
    expect(rows.offsetOf(4)).toBe(110);
    expect(rows.total).toBe(2030);
    expect(rows.indexAt(59)).toBe(2);
    expect(rows.indexAt(60)).toBe(3);
    expect(rows.indexAt(109)).toBe(3);
    expect(rows.indexAt(110)).toBe(4);
    expect(rows.indexAt(135)).toBe(5);
  });

  it("never lands on a hidden index", () => {
    const rows = new AxisGeometry(10, 20);
    rows.setSize(0, 0);
    rows.setSize(4, 0);
    expect(rows.indexAt(0)).toBe(1);
    expect(rows.offsetOf(4)).toBe(rows.offsetOf(5));
    expect(rows.indexAt(rows.offsetOf(5))).toBe(5);
    rows.setSize(9, 0);
    expect(rows.indexAt(rows.total + 5)).toBe(8);
  });

  it("removes an override when set back to the default", () => {
    const cols = new AxisGeometry(16_384, 64);
    cols.setSize(10, 100);
    cols.setSize(5, 30);
    cols.setSize(10, 64);
    expect(cols.sizeOf(10)).toBe(64);
    expect(cols.total).toBe(16_384 * 64 - 34);
  });

  it("matches a brute-force model under random edits", () => {
    const count = 500;
    const axis = new AxisGeometry(count, 20);
    const sizes = new Array<number>(count).fill(20);
    let seed = 7;
    const rand = (n: number) => {
      seed = (seed * 1103515245 + 12345) % 2 ** 31;
      return seed % n;
    };
    for (let step = 0; step < 300; step++) {
      const i = rand(count);
      const s = [0, 20, 5, 47, 120][rand(5)]!;
      axis.setSize(i, s);
      sizes[i] = s;
    }
    let offset = 0;
    for (let i = 0; i < count; i++) {
      expect(axis.offsetOf(i)).toBe(offset);
      if (sizes[i]! > 0) expect(axis.indexAt(offset)).toBe(i);
      offset += sizes[i]!;
    }
    expect(axis.total).toBe(offset);
  });
});
