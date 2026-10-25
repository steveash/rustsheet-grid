import { readFile } from "node:fs/promises";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { join, normalize } from "node:path";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { PNG } from "pngjs";
import { WebSocketServer } from "ws";
import type { BridgeHub } from "./bridge.ts";
import type { GymConfig } from "./config.ts";
import type { ExcelSession } from "./excel.ts";
import { buildMcpServer, type Mutex } from "./mcp.ts";

/** Files of the bridge add-in the gym serves. Anything else under /addin/ is a 404. */
const ADDIN_FILES: Record<string, string> = {
  "taskpane.html": "text/html; charset=utf-8",
  "taskpane.js": "text/javascript; charset=utf-8",
  "ops.js": "text/javascript; charset=utf-8",
};

export interface Routes {
  kind: "health" | "mcp" | "smoke" | "addin" | "bridge" | "notfound";
  file?: string;
}

/**
 * Classifies a request path. Only /healthz is public; every other route sits under the secret
 * prefix /s/<token>/, compared in constant time.
 */
export function route(path: string, token: string): Routes {
  if (path === "/healthz") return { kind: "health" };
  const m = /^\/s\/([^/]+)(\/.*)$/.exec(path);
  if (!m || !safeEqual(m[1]!, token)) return { kind: "notfound" };
  const rest = m[2]!;
  if (rest === "/mcp") return { kind: "mcp" };
  if (rest === "/smoke") return { kind: "smoke" };
  if (rest === "/bridge") return { kind: "bridge" };
  const a = /^\/addin\/([A-Za-z0-9_.-]+)$/.exec(rest);
  if (a && (a[1]! in ADDIN_FILES || a[1] === "manifest.xml" || a[1] === "icon.png")) return { kind: "addin", file: a[1]! };
  return { kind: "notfound" };
}

function safeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

function icon(): Buffer {
  const png = new PNG({ width: 80, height: 80 });
  for (let i = 0; i < png.data.length; i += 4) {
    png.data[i] = 33;
    png.data[i + 1] = 115;
    png.data[i + 2] = 70;
    png.data[i + 3] = 255;
  }
  return PNG.sync.write(png);
}

async function readBody(req: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    size += (chunk as Buffer).length;
    if (size > 50 * 1024 * 1024) throw new Error("request body too large");
    chunks.push(chunk as Buffer);
  }
  const raw = Buffer.concat(chunks).toString("utf8");
  return raw ? JSON.parse(raw) : undefined;
}

export function startHttp(cfg: GymConfig, addinDir: string, excel: ExcelSession, bridge: BridgeHub, lock: Mutex): Server {
  const base = `${cfg.publicOrigin}/s/${cfg.token}/addin`;
  const wss = new WebSocketServer({ noServer: true, maxPayload: 50 * 1024 * 1024 });

  const send = (res: ServerResponse, status: number, type: string, body: string | Buffer) => {
    res.writeHead(status, { "content-type": type, "cache-control": "no-store" });
    res.end(body);
  };

  const server = createServer(async (req, res) => {
    const url = new URL(req.url ?? "/", "http://gym");
    const r = route(url.pathname, cfg.token);
    try {
      switch (r.kind) {
        case "health":
          return send(res, 200, "application/json", JSON.stringify({ ok: true, bridgeConnected: bridge.connected }));
        case "smoke": {
          const result = await lock.run(() => excel.smoke());
          return send(res, result.ok ? 200 : 503, "application/json", JSON.stringify(result));
        }
        case "addin": {
          if (r.file === "icon.png") return send(res, 200, "image/png", icon());
          if (r.file === "manifest.xml") {
            const xml = (await readFile(join(addinDir, "manifest.template.xml"), "utf8"))
              .replaceAll("{{BASE}}", base)
              .replaceAll("{{ORIGIN}}", cfg.publicOrigin);
            return send(res, 200, "application/xml; charset=utf-8", xml);
          }
          const file = normalize(r.file!);
          return send(res, 200, ADDIN_FILES[file]!, await readFile(join(addinDir, file)));
        }
        case "mcp": {
          // Stateless: a fresh server and transport per request; the Excel session is shared.
          const mcp = buildMcpServer(excel, bridge, lock);
          const transport = new StreamableHTTPServerTransport({});
          res.on("close", () => {
            void transport.close();
            void mcp.close();
          });
          await mcp.connect(transport);
          const body = req.method === "POST" ? await readBody(req) : undefined;
          await transport.handleRequest(req, res, body);
          return;
        }
        default:
          return send(res, 404, "text/plain", "not found");
      }
    } catch (e) {
      if (!res.headersSent) send(res, 500, "text/plain", `error: ${(e as Error).message}`);
    }
  });

  server.on("upgrade", (req, socket, head) => {
    const url = new URL(req.url ?? "/", "http://gym");
    const fromLoopback = req.socket.remoteAddress === "127.0.0.1" || req.socket.remoteAddress === "::1" || req.socket.remoteAddress === "::ffff:127.0.0.1";
    if (route(url.pathname, cfg.token).kind !== "bridge" || !fromLoopback) {
      socket.destroy();
      return;
    }
    wss.handleUpgrade(req, socket, head, (ws) => bridge.attach(ws));
  });

  server.listen(cfg.port);
  return server;
}
