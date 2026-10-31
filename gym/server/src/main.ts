import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { BridgeHub } from "./bridge.ts";
import { loadConfig } from "./config.ts";
import { ExcelSession } from "./excel.ts";
import { startHttp } from "./http.ts";
import { Mutex } from "./mcp.ts";

const cfg = loadConfig();
const addinDir = process.env.GYM_ADDIN_DIR ?? join(dirname(fileURLToPath(import.meta.url)), "../../bridge-addin");
const bridge = new BridgeHub();
const excel = new ExcelSession(cfg, bridge);
const lock = new Mutex();

startHttp(cfg, addinDir, excel, bridge, lock);
console.log(`gym listening on :${cfg.port}`);

// Open Excel eagerly so the first tool call is fast; failures are retried on demand.
lock.run(() => excel.ensureOpen()).catch((e) => console.error(`excel did not open: ${(e as Error).message}`));

const stop = async () => {
  await excel.close();
  process.exit(0);
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
