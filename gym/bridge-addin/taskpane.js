// Gym bridge task pane: connects to the gym server inside the same container and runs the ops it
// sends. The page is served at /s/<token>/addin/taskpane.html; the bridge socket lives at
// /s/<token>/bridge on 127.0.0.1, so each gym container talks only to its own Excel tab.
import { dispatch, makeOps } from "./ops.js";

const statusEl = document.getElementById("status");
const logEl = document.getElementById("log");
const log = (line) => {
  logEl.textContent = `${new Date().toISOString().slice(11, 19)} ${line}\n${logEl.textContent}`.slice(0, 4000);
};
const setStatus = (s) => {
  statusEl.textContent = s;
};

const prefix = location.pathname.replace(/\/addin\/[^/]*$/, "");
const port = new URLSearchParams(location.search).get("bridgePort") ?? "8787";
const bridgeUrl = `ws://127.0.0.1:${port}${prefix}/bridge`;

Office.onReady(async (info) => {
  if (info.host !== Office.HostType.Excel) {
    setStatus("not Excel");
    return;
  }
  // Reopen this pane whenever the workbook opens, so the gym never has to click the ribbon.
  Office.context.document.settings.set("Office.AutoShowTaskpaneWithDocument", true);
  Office.context.document.settings.saveAsync();

  const ops = makeOps(Excel);
  let delay = 500;
  const connect = () => {
    const ws = new WebSocket(bridgeUrl);
    ws.onopen = () => {
      delay = 500;
      setStatus("connected");
      ws.send(JSON.stringify({ hello: "bridge", version: 1 }));
    };
    ws.onmessage = async (ev) => {
      let request;
      try {
        request = JSON.parse(ev.data);
      } catch {
        return;
      }
      log(`${request.op}`);
      ws.send(JSON.stringify(await dispatch(ops, request)));
    };
    ws.onclose = () => {
      setStatus("reconnecting");
      setTimeout(connect, delay);
      delay = Math.min(delay * 2, 10_000);
    };
  };
  connect();
});
