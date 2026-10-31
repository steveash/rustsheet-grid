/**
 * Pixel geometry of one axis (rows or columns): a default size with sparse per-index overrides.
 *
 * A financial model may resize a few hundred rows of a billion, so sizes are stored only where
 * they differ from the default. Both directions — "where does index i start" and "which index is
 * at pixel p" — cost O(log k) for k overrides, independent of the axis length. Hidden rows and
 * columns are overrides of size 0.
 *
 * Offsets can exceed 2^53 only past ~10^14 rows at default size, far beyond any sheet, so plain
 * numbers are exact here.
 */
export class AxisGeometry {
  readonly count: number;
  readonly defaultSize: number;
  /** Override indexes, ascending. */
  private indexes: number[] = [];
  /** Size of each override, parallel to `indexes`. */
  private sizes: number[] = [];
  /** `deltas[k]` = sum of (size - default) over overrides 0..k-1; length k + 1. */
  private deltas: number[] = [0];

  constructor(count: number, defaultSize: number) {
    if (!Number.isSafeInteger(count) || count <= 0) throw new RangeError("count must be a positive integer");
    if (!(defaultSize > 0)) throw new RangeError("defaultSize must be positive");
    this.count = count;
    this.defaultSize = defaultSize;
  }

  /** Total length of the axis in pixels. */
  get total(): number {
    return this.count * this.defaultSize + this.deltas[this.deltas.length - 1]!;
  }

  /** Sets the size of `index`; `defaultSize` removes its override. */
  setSize(index: number, size: number): void {
    this.checkIndex(index);
    if (!(size >= 0)) throw new RangeError("size must be >= 0");
    const k = this.lowerBound(index);
    const present = this.indexes[k] === index;
    if (size === this.defaultSize) {
      if (present) {
        this.indexes.splice(k, 1);
        this.sizes.splice(k, 1);
      }
    } else if (present) {
      this.sizes[k] = size;
    } else {
      this.indexes.splice(k, 0, index);
      this.sizes.splice(k, 0, size);
    }
    this.rebuildDeltas(k);
  }

  /** Size of `index` in pixels. */
  sizeOf(index: number): number {
    this.checkIndex(index);
    const k = this.lowerBound(index);
    return this.indexes[k] === index ? this.sizes[k]! : this.defaultSize;
  }

  /** Pixel offset where `index` starts. `offsetOf(count)` is the total length. */
  offsetOf(index: number): number {
    if (!Number.isInteger(index) || index < 0 || index > this.count) throw new RangeError(`index ${index} out of range`);
    // Overrides strictly before `index` shift its start.
    return index * this.defaultSize + this.deltas[this.lowerBound(index)]!;
  }

  /**
   * The index whose span contains `offset` (clamped to the axis). Zero-size indexes are never
   * returned, as a click never lands on a hidden row.
   */
  indexAt(offset: number): number {
    if (offset <= 0) return this.firstVisibleFrom(0);
    if (offset >= this.total) return this.lastVisible();
    // Binary search over overrides for the last one starting at or before `offset`, then step
    // through the default-sized run that follows it.
    let lo = 0;
    let hi = this.indexes.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (this.offsetOf(this.indexes[mid]!) <= offset) lo = mid + 1;
      else hi = mid;
    }
    if (lo > 0) {
      const k = lo - 1;
      const idx = this.indexes[k]!;
      const start = this.offsetOf(idx);
      if (offset < start + this.sizes[k]!) return idx;
      const after = Math.floor((offset - (start + this.sizes[k]!)) / this.defaultSize);
      return Math.min(idx + 1 + after, this.count - 1);
    }
    return Math.min(Math.floor(offset / this.defaultSize), this.count - 1);
  }

  private firstVisibleFrom(index: number): number {
    let i = index;
    while (i < this.count - 1 && this.sizeOf(i) === 0) i++;
    return i;
  }

  private lastVisible(): number {
    let i = this.count - 1;
    while (i > 0 && this.sizeOf(i) === 0) i--;
    return i;
  }

  /** First position in `indexes` whose value is >= `index`. */
  private lowerBound(index: number): number {
    let lo = 0;
    let hi = this.indexes.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (this.indexes[mid]! < index) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  private rebuildDeltas(from: number): void {
    this.deltas.length = this.indexes.length + 1;
    for (let k = from; k < this.indexes.length; k++) {
      this.deltas[k + 1] = this.deltas[k]! + (this.sizes[k]! - this.defaultSize);
    }
  }

  private checkIndex(index: number): void {
    if (!Number.isInteger(index) || index < 0 || index >= this.count) throw new RangeError(`index ${index} out of range`);
  }
}
