import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { BridgeHub } from "./bridge.ts";
import type { ExcelSession } from "./excel.ts";
import { Step } from "./steps.ts";

/** Serializes tool calls: there is one Excel tab, and interleaved actions would corrupt probes. */
export class Mutex {
  private tail: Promise<unknown> = Promise.resolve();
  run<T>(fn: () => Promise<T>): Promise<T> {
    const next = this.tail.then(fn, fn);
    this.tail = next.catch(() => {});
    return next;
  }
}

const BridgeOp = z.object({
  op: z.string().describe("ping, reset, loadXlsx, setValues, setFormulas, setNumberFormat, setFormat, addConditionalFormat, setSize, freeze, select, read, geometry, selection, eval"),
  args: z.record(z.string(), z.unknown()).optional(),
});

const text = (value: unknown) => ({ type: "text" as const, text: typeof value === "string" ? value : JSON.stringify(value, null, 2) });
const image = (png: Buffer) => ({ type: "image" as const, data: png.toString("base64"), mimeType: "image/png" });

/**
 * The gym's MCP surface: primitives only. Probe catalogs, scenarios and diffing live with the
 * caller, so new kinds of probes never need a gym deploy (CLAUDE.md, directive 5).
 */
export function buildMcpServer(excel: ExcelSession, bridge: BridgeHub, lock: Mutex): McpServer {
  const server = new McpServer({ name: "rustsheet-gym", version: "0.1.0" });
  const guarded = async (fn: () => Promise<{ content: Array<ReturnType<typeof text> | ReturnType<typeof image>> }>) => {
    try {
      return await lock.run(fn);
    } catch (e) {
      return { content: [text(`error: ${(e as Error).message ?? e}`)], isError: true };
    }
  };

  server.registerTool(
    "gym_status",
    { description: "Whether Excel is open and the bridge add-in is connected. deep=true runs the deploy smoke check." , inputSchema: { deep: z.boolean().optional() } },
    async ({ deep }) => guarded(async () => ({ content: [text(deep ? await excel.smoke() : { bridgeConnected: bridge.connected })] })),
  );

  server.registerTool(
    "excel_reset",
    {
      description:
        "Fresh state in real Excel for the web: one empty sheet (or the sheets of an .xlsx given as base64), A1 selected, view at the top-left, screen calibration redone.",
      inputSchema: { xlsxBase64: z.string().optional(), sheetName: z.string().optional() },
    },
    async ({ xlsxBase64, sheetName }) =>
      guarded(async () => {
        await excel.ensureOpen();
        const result = xlsxBase64 ? await bridge.call("loadXlsx", { base64: xlsxBase64 }, 120_000) : await bridge.call("reset", { name: sheetName ?? "Gym" });
        excel.invalidateMap();
        const map = await excel.calibrate();
        return { content: [text({ result, screenMap: map })] };
      }),
  );

  server.registerTool(
    "excel_bridge",
    {
      description:
        "Runs bridge ops inside Excel via Office.js, in order, stopping at the first failure. Ops set state (setValues, setFormat, addConditionalFormat, setSize, freeze, select, ...) and read truth (read, geometry, selection). eval runs arbitrary Office.js: args.code is an async function body given (Excel, context) that returns JSON.",
      inputSchema: { ops: z.array(BridgeOp).min(1).max(200) },
    },
    async ({ ops }) =>
      guarded(async () => {
        await excel.ensureOpen();
        const results: unknown[] = [];
        for (const { op, args } of ops) {
          results.push(await bridge.call(op, args ?? {}));
          if (op === "setSize" || op === "freeze" || op === "reset" || op === "loadXlsx") excel.invalidateMap();
        }
        return { content: [text(results)] };
      }),
  );

  server.registerTool(
    "excel_act",
    {
      description:
        "Real keyboard and mouse input in Excel: click a cell, press keys (Playwright syntax, e.g. Control+ArrowDown), type text, drag (handle 'fill' drags the fill handle), wait. Returns the selection afterwards.",
      inputSchema: { steps: z.array(Step).min(1).max(100) },
    },
    async ({ steps }) =>
      guarded(async () => {
        await excel.act(steps);
        const selection = await bridge.call("selection");
        return { content: [text({ selection })] };
      }),
  );

  server.registerTool(
    "excel_observe",
    {
      description:
        "A settled screenshot of a range (with a margin) plus each cell's pixel box in the image, and what Excel holds for the range (values, display text, formulas, number formats) and the selection. address omitted = the whole browser viewport. Geometry assumes the view still shows A1 at the top-left.",
      inputSchema: { address: z.string().optional(), margin: z.number().int().min(0).max(200).optional() },
    },
    async ({ address, margin }) =>
      guarded(async () => {
        if (!address) {
          const { png, stable } = await excel.screenshot();
          return { content: [image(png), text({ stable })] };
        }
        const shot = await excel.observe(address, margin ?? 8);
        const truth = await bridge.call("read", { address });
        const selection = await bridge.call("selection");
        return { content: [image(shot.png), text({ stable: shot.stable, clip: shot.clip, cells: shot.cells, truth, selection })] };
      }),
  );

  return server;
}
