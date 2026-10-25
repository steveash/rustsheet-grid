import { createServer, type AddressInfo } from "node:net";
import { PNG } from "pngjs";
import { describe, expect, it } from "vitest";
import { WebSocket, WebSocketServer } from "ws";
import { BridgeHub, BridgeOpError } from "./bridge.ts";
import { loadConfig } from "./config.ts";
import { cellRects, screenMapFrom, toScreen } from "./geometry.ts";
import { route } from "./http.ts";
import { captureStable, findColorBox, isUniform } from "./image.ts";
import { Mutex } from "./mcp.ts";
import { referencedCells, Step } from "./steps.ts";

const TOKEN = "t0ken-abcdefghijklmnopqrstuvwxyz";

function png(width: number, height: number, paint: (x: number, y: number) => [number, number, number]): Buffer {
  const img = new PNG({ width, height });
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++) {
      const [r, g, b] = paint(x, y);
      const i = (y * width + x) * 4;
      img.data[i] = r;
      img.data[i + 1] = g;
      img.data[i + 2] = b;
      img.data[i + 3] = 255;
    }
  return PNG.sync.write(img);
}

describe("calibration", () => {
  it("finds the marker block and maps points to pixels", () => {
    // A1:C3 painted at x 50..241, y 120..179 (3 columns of 64 px, 3 rows of 20 px).
    const shot = png(400, 300, (x, y) => (x >= 50 && x < 242 && y >= 120 && y < 180 ? [255, 0, 255] : [255, 255, 255]));
    const box = findColorBox(shot, { r: 255, g: 0, b: 255 });
    expect(box).toEqual({ x: 50, y: 120, width: 192, height: 60 });
    // In points: columns 48 pt wide, rows 15 pt high, so 4/3 px per point.
    const map = screenMapFrom(box!, { left: 0, top: 0, width: 144, height: 45 });
    expect(map.scaleX).toBeCloseTo(4 / 3);
    expect(toScreen(map, { left: 48, top: 15, width: 48, height: 15 })).toEqual({ x: 114, y: 140, width: 64, height: 20 });
    const rects = cellRects(map, { left: 0, top: 0, width: 96, height: 15, columns: [{ left: 0, width: 48 }, { left: 48, width: 48 }], rows: [{ top: 0, height: 15 }] });
    expect(rects[0]!.map((r) => r.x)).toEqual([50, 114]);
  });

  it("reports a missing marker and a blank frame", () => {
    const white = png(10, 10, () => [255, 255, 255]);
    expect(findColorBox(white, { r: 255, g: 0, b: 255 })).toBeUndefined();
    expect(isUniform(white)).toBe(true);
    expect(isUniform(png(10, 10, (x) => (x === 3 ? [0, 0, 0] : [255, 255, 255])))).toBe(false);
  });
});

describe("captureStable", () => {
  it("returns once two consecutive frames match", async () => {
    const frames = [png(4, 4, () => [1, 1, 1]), png(4, 4, () => [2, 2, 2]), png(4, 4, () => [3, 3, 3]), png(4, 4, () => [3, 3, 3])];
    let i = 0;
    const r = await captureStable(async () => frames[Math.min(i++, frames.length - 1)]!, { intervalMs: 1 });
    expect(r.stable).toBe(true);
    expect(r.frames).toBe(4);
  });

  it("gives up after the timeout", async () => {
    let n = 0;
    const r = await captureStable(async () => png(2, 2, () => [n++ % 256, 0, 0]), { intervalMs: 1, timeoutMs: 20 });
    expect(r.stable).toBe(false);
  });
});

describe("routes", () => {
  it("keeps everything but health behind the secret prefix", () => {
    expect(route("/healthz", TOKEN).kind).toBe("health");
    expect(route(`/s/${TOKEN}/mcp`, TOKEN).kind).toBe("mcp");
    expect(route(`/s/${TOKEN}/bridge`, TOKEN).kind).toBe("bridge");
    expect(route(`/s/${TOKEN}/addin/taskpane.html`, TOKEN)).toEqual({ kind: "addin", file: "taskpane.html" });
    expect(route(`/s/${TOKEN}/addin/manifest.xml`, TOKEN).kind).toBe("addin");
    expect(route(`/s/wrong-token-abcdefghijklmnopqrst/mcp`, TOKEN).kind).toBe("notfound");
    expect(route(`/s/${TOKEN}/addin/../../etc/passwd`, TOKEN).kind).toBe("notfound");
    expect(route(`/s/${TOKEN}/addin/secret.env`, TOKEN).kind).toBe("notfound");
    expect(route("/mcp", TOKEN).kind).toBe("notfound");
  });

  it("refuses a weak token", () => {
    const env = { GYM_PUBLIC_ORIGIN: "https://x", EXCEL_WORKBOOK_URL: "https://y", M365_USERNAME: "u", M365_PASSWORD: "p" };
    expect(() => loadConfig({ ...env, GYM_TOKEN: "short" })).toThrow(/GYM_TOKEN/);
    expect(loadConfig({ ...env, GYM_TOKEN: TOKEN }).viewport).toEqual({ width: 1600, height: 1000 });
  });
});

describe("steps", () => {
  it("validates steps and collects the cells they touch", () => {
    const steps = [
      { type: "click", cell: "B3" },
      { type: "press", keys: "Control+ArrowDown" },
      { type: "drag", from: "B3", to: "B9", handle: "fill" },
    ].map((s) => Step.parse(s));
    expect(referencedCells(steps)).toEqual(["B3", "B9"]);
    expect(() => Step.parse({ type: "press", keys: "" })).toThrow();
  });
});

describe("Mutex", () => {
  it("runs calls one at a time and survives failures", async () => {
    const m = new Mutex();
    const order: string[] = [];
    const slow = (tag: string, ms: number) => m.run(async () => {
      order.push(`start ${tag}`);
      await new Promise((r) => setTimeout(r, ms));
      order.push(`end ${tag}`);
    });
    const failing = m.run(async () => {
      throw new Error("x");
    });
    await Promise.all([slow("a", 10), failing.catch(() => {}), slow("b", 1)]);
    expect(order).toEqual(["start a", "end a", "start b", "end b"]);
  });
});

describe("BridgeHub", () => {
  async function pair() {
    const port = await new Promise<number>((resolve) => {
      const s = createServer().listen(0, () => {
        const p = (s.address() as AddressInfo).port;
        s.close(() => resolve(p));
      });
    });
    const wss = new WebSocketServer({ port });
    const hub = new BridgeHub();
    wss.on("connection", (ws) => hub.attach(ws));
    const addin = new WebSocket(`ws://127.0.0.1:${port}`);
    await hub.waitForConnection(2_000);
    return { hub, addin, wss };
  }

  it("matches replies to calls and surfaces op errors", async () => {
    const { hub, addin, wss } = await pair();
    addin.on("message", (data) => {
      const req = JSON.parse(String(data));
      const reply = req.op === "boom" ? { id: req.id, ok: false, error: { message: "bad range" } } : { id: req.id, ok: true, result: { echo: req.args } };
      addin.send(JSON.stringify(reply));
    });
    await expect(hub.call("read", { address: "A1" })).resolves.toEqual({ echo: { address: "A1" } });
    await expect(hub.call("boom")).rejects.toBeInstanceOf(BridgeOpError);
    addin.close();
    wss.close();
  });

  it("fails pending calls when the add-in disconnects, and times out silent ones", async () => {
    const { hub, addin, wss } = await pair();
    await expect(hub.call("ping", {}, 50)).rejects.toThrow(/timed out/);
    const pending = hub.call("ping", {}, 5_000);
    addin.close();
    await expect(pending).rejects.toThrow(/disconnected/);
    expect(hub.connected).toBe(false);
    wss.close();
  });
});
