import { existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { chromium, type Browser, type BrowserContext, type Page } from "playwright";
import type { BridgeHub } from "./bridge.ts";
import type { GymConfig } from "./config.ts";
import { center, cellRects, pad, screenMapFrom, toScreen, type PointGeometry, type ScreenMap } from "./geometry.ts";
import { captureStable, findColorBox, isUniform, type Rect } from "./image.ts";
import { isLoginUrl, signIn } from "./login.ts";
import { referencedCells, type Step } from "./steps.ts";

/** Marker color for calibration; Excel's own palette never draws pure magenta. */
const MARKER = { r: 255, g: 0, b: 255 };
const MARKER_RANGE = "A1:C3";
const CALIBRATION_SHEET = "gym-calibrate";

/** The one Excel-for-the-web tab the gym drives, plus the bridge add-in inside it. */
export class ExcelSession {
  private browser: Browser | undefined;
  private context: BrowserContext | undefined;
  private page: Page | undefined;
  private screenMap: ScreenMap | undefined;
  private readonly cfg: GymConfig;
  private readonly bridge: BridgeHub;

  constructor(cfg: GymConfig, bridge: BridgeHub) {
    this.cfg = cfg;
    this.bridge = bridge;
  }

  private get statePath(): string {
    return join(this.cfg.stateDir, "storage-state.json");
  }

  /** Opens Excel on the gym workbook if it is not already open with the bridge attached. */
  async ensureOpen(): Promise<Page> {
    if (this.page && !this.page.isClosed() && this.bridge.connected) return this.page;
    if (this.page && !this.page.isClosed()) {
      // Tab alive but the add-in dropped: a reload reopens the pane (it auto-shows).
      await this.page.reload({ waitUntil: "domcontentloaded" });
      await this.bridge.waitForConnection(90_000);
      this.screenMap = undefined;
      return this.page;
    }
    await this.close();
    mkdirSync(this.cfg.stateDir, { recursive: true });
    this.browser = await chromium.launch({
      args: [
        // The add-in page (public origin) connects to its gym on 127.0.0.1; allow that hop.
        "--disable-features=LocalNetworkAccessChecks,PrivateNetworkAccessSendPreflights,PrivateNetworkAccessRespectPreflightResults,BlockInsecurePrivateNetworkRequests",
      ],
    });
    this.context = await this.browser.newContext({
      viewport: this.cfg.viewport,
      deviceScaleFactor: this.cfg.deviceScaleFactor,
      locale: "en-US",
      timezoneId: "UTC",
      colorScheme: "light",
      ...(existsSync(this.statePath) ? { storageState: this.statePath } : {}),
    });
    const page = await this.context.newPage();
    this.page = page;
    await page.goto(this.cfg.workbookUrl, { waitUntil: "domcontentloaded" });
    if (isLoginUrl(page.url())) {
      await signIn(page, this.cfg);
      await this.context.storageState({ path: this.statePath });
    }
    await this.bridge.waitForConnection(120_000);
    this.screenMap = undefined;
    return page;
  }

  async close(): Promise<void> {
    await this.browser?.close().catch(() => {});
    this.browser = undefined;
    this.context = undefined;
    this.page = undefined;
    this.screenMap = undefined;
  }

  /**
   * Measures where sheet points land on screen: on a scratch sheet, paints MARKER_RANGE magenta,
   * finds it in a screenshot, then deletes the scratch sheet and returns to the active one.
   * Must run with the view at the top-left (after reset or select A1).
   */
  async calibrate(): Promise<ScreenMap> {
    const page = await this.ensureOpen();
    const marked = await this.bridge.call<{ left: number; top: number; width: number; height: number; back: string }>("eval", {
      code: `
        const back = context.workbook.worksheets.getActiveWorksheet();
        back.load("name");
        const old = context.workbook.worksheets.getItemOrNullObject(${JSON.stringify(CALIBRATION_SHEET)});
        await context.sync();
        if (!old.isNullObject) { old.delete(); await context.sync(); }
        const s = context.workbook.worksheets.add(${JSON.stringify(CALIBRATION_SHEET)});
        s.activate();
        const r = s.getRange(${JSON.stringify(MARKER_RANGE)});
        r.format.fill.color = "#FF00FF";
        s.getRange("H12").select();
        r.load("left,top,width,height");
        await context.sync();
        return { left: r.left, top: r.top, width: r.width, height: r.height, back: back.name };`,
    });
    try {
      const { png } = await captureStable(() => page.screenshot());
      const box = findColorBox(png, MARKER);
      if (!box) throw new Error("calibration marker not visible: is the grid covered by a dialog or pane?");
      this.screenMap = screenMapFrom(box, marked);
      return this.screenMap;
    } finally {
      await this.bridge.call("eval", {
        code: `
          const back = context.workbook.worksheets.getItem(${JSON.stringify(marked.back)});
          back.activate();
          context.workbook.worksheets.getItem(${JSON.stringify(CALIBRATION_SHEET)}).delete();
          await context.sync();
          return {};`,
      });
    }
  }

  private async map(): Promise<ScreenMap> {
    return this.screenMap ?? this.calibrate();
  }

  /** Forgets the calibration (after anything that may scroll or zoom). */
  invalidateMap(): void {
    this.screenMap = undefined;
  }

  /** Performs user actions with real mouse and keyboard input. */
  async act(steps: Step[]): Promise<void> {
    const page = await this.ensureOpen();
    const map = await this.map();
    const cells = referencedCells(steps);
    const rects = new Map<string, Rect>();
    for (const cell of cells) {
      const g = await this.bridge.call<PointGeometry>("geometry", { address: cell });
      rects.set(cell, toScreen(map, g));
    }
    const rectOf = (cell: string): Rect => {
      const r = rects.get(cell);
      if (!r) throw new Error(`no geometry for ${cell}`);
      return r;
    };
    for (const step of steps) {
      switch (step.type) {
        case "click": {
          const p = step.cell ? center(rectOf(step.cell)) : step.at;
          if (!p) throw new Error("click needs a cell or a point");
          for (const m of step.modifiers ?? []) await page.keyboard.down(m);
          await page.mouse.click(p.x, p.y, { clickCount: step.count ?? 1 });
          for (const m of step.modifiers ?? []) await page.keyboard.up(m);
          break;
        }
        case "press":
          for (let i = 0; i < (step.repeat ?? 1); i++) await page.keyboard.press(step.keys);
          break;
        case "type":
          await page.keyboard.type(step.text, { delay: step.delayMs ?? 20 });
          break;
        case "drag": {
          const from = rectOf(step.from);
          const start = step.handle === "fill" ? { x: from.x + from.width - 1, y: from.y + from.height - 1 } : center(from);
          const end = center(rectOf(step.to));
          await page.mouse.move(start.x, start.y);
          await page.mouse.down();
          await page.mouse.move(end.x, end.y, { steps: 12 });
          await page.mouse.up();
          break;
        }
        case "wait":
          await page.waitForTimeout(step.ms);
          break;
      }
    }
  }

  /**
   * A settled screenshot of `address` (plus `margin` pixels around it, to show borders, overflow
   * and selection outlines) with each cell's pixel rectangle relative to the image.
   */
  async observe(address: string, margin = 8): Promise<{ png: Buffer; stable: boolean; cells: Rect[][]; clip: Rect }> {
    const page = await this.ensureOpen();
    const map = await this.map();
    const g = await this.bridge.call<PointGeometry>("geometry", { address });
    const clip = pad(toScreen(map, g), margin, this.cfg.viewport);
    const { png, stable } = await captureStable(() => page.screenshot({ clip }));
    const cells = cellRects(map, g).map((row) => row.map((r) => ({ ...r, x: r.x - clip.x, y: r.y - clip.y })));
    return { png, stable, cells, clip };
  }

  /** A settled screenshot of the whole browser viewport. */
  async screenshot(): Promise<{ png: Buffer; stable: boolean }> {
    const page = await this.ensureOpen();
    return captureStable(() => page.screenshot());
  }

  /** End-to-end check used by deploys: Excel opens, the bridge answers, the grid paints. */
  async smoke(): Promise<{ ok: boolean; detail: string }> {
    try {
      await this.ensureOpen();
      await this.bridge.call("ping");
      await this.bridge.call("read", { address: "A1" });
      const { png } = await this.screenshot();
      if (isUniform(png)) return { ok: false, detail: "screenshot is blank" };
      return { ok: true, detail: "excel open, bridge answering, grid painted" };
    } catch (e) {
      return { ok: false, detail: String((e as Error).message ?? e) };
    }
  }
}
