import type { WebSocket } from "ws";

interface Pending {
  op: string;
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
  timer: NodeJS.Timeout;
}

/** An error the add-in reported while running an op inside Excel. */
export class BridgeOpError extends Error {
  readonly detail: unknown;
  constructor(op: string, detail: { message?: string } & Record<string, unknown>) {
    super(`bridge op ${op} failed: ${detail.message ?? "unknown error"}`);
    this.detail = detail;
  }
}

/**
 * The gym side of the bridge: holds the add-in's socket and turns ops into request/response calls.
 * Exactly one add-in is attached at a time; a new connection replaces the old one (the pane
 * reloads whenever Excel reloads the workbook).
 */
export class BridgeHub {
  private socket: WebSocket | undefined;
  private nextId = 1;
  private pending = new Map<number, Pending>();
  private waiters: Array<() => void> = [];

  get connected(): boolean {
    return this.socket !== undefined;
  }

  attach(socket: WebSocket): void {
    this.detach(new Error("bridge replaced by a new connection"));
    this.socket = socket;
    socket.on("message", (data) => this.onMessage(String(data)));
    socket.on("close", () => {
      if (this.socket === socket) this.detach(new Error("bridge disconnected"));
    });
    for (const w of this.waiters.splice(0)) w();
  }

  /** Resolves once an add-in is attached, or rejects after `timeoutMs`. */
  waitForConnection(timeoutMs: number): Promise<void> {
    if (this.socket) return Promise.resolve();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiters = this.waiters.filter((w) => w !== done);
        reject(new Error(`bridge add-in did not connect within ${timeoutMs} ms`));
      }, timeoutMs);
      const done = () => {
        clearTimeout(timer);
        resolve();
      };
      this.waiters.push(done);
    });
  }

  /** Runs `op` inside Excel and returns its result. */
  call<T = unknown>(op: string, args: Record<string, unknown> = {}, timeoutMs = 30_000): Promise<T> {
    const socket = this.socket;
    if (!socket) return Promise.reject(new Error("bridge add-in is not connected"));
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`bridge op ${op} timed out after ${timeoutMs} ms`));
      }, timeoutMs);
      this.pending.set(id, { op, resolve: resolve as (v: unknown) => void, reject, timer });
      socket.send(JSON.stringify({ id, op, args }));
    });
  }

  private onMessage(text: string): void {
    let msg: { id?: number; ok?: boolean; result?: unknown; error?: { message?: string } };
    try {
      msg = JSON.parse(text);
    } catch {
      return;
    }
    if (typeof msg.id !== "number") return; // hello and other notices
    const p = this.pending.get(msg.id);
    if (!p) return;
    this.pending.delete(msg.id);
    clearTimeout(p.timer);
    if (msg.ok) p.resolve(msg.result);
    else p.reject(new BridgeOpError(p.op, msg.error ?? {}));
  }

  private detach(reason: Error): void {
    this.socket = undefined;
    for (const [id, p] of this.pending) {
      clearTimeout(p.timer);
      p.reject(reason);
      this.pending.delete(id);
    }
  }
}
