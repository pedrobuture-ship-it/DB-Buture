// Minimal Chrome DevTools Protocol driver (no dependencies; uses Node's built-in WebSocket).
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function launchBrowser({ port = 9333, width = 1500, height = 950 } = {}) {
  const profile = mkdtempSync(join(tmpdir(), "buture-chrome-"));
  const proc = spawn(
    "chromium",
    [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profile}`,
      `--window-size=${width},${height}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  let targets;
  for (let i = 0; i < 50; i++) {
    try {
      targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      if (targets.some((t) => t.type === "page")) break;
    } catch {
      /* not up yet */
    }
    await sleep(200);
  }
  const page = targets?.find((t) => t.type === "page");
  if (!page) throw new Error("chromium did not start");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pending = new Map();
  const logs = [];
  ws.onmessage = (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(msg.error.message));
      else resolve(msg.result);
    } else if (
      msg.method === "Runtime.consoleAPICalled" &&
      ["error", "warning"].includes(msg.params.type)
    ) {
      logs.push(
        `${msg.params.type}: ${msg.params.args.map((a) => a.value ?? a.description).join(" ")}`,
      );
    } else if (msg.method === "Runtime.exceptionThrown") {
      logs.push(
        `exception: ${msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text}`,
      );
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const n = ++id;
      pending.set(n, { resolve, reject });
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  await send("Page.enable");
  await send("Runtime.enable");

  const api = {
    logs,
    send,
    async inject(file) {
      await send("Page.addScriptToEvaluateOnNewDocument", { source: readFileSync(file, "utf8") });
    },
    async goto(url) {
      await send("Page.navigate", { url });
      await sleep(600);
    },
    async eval(expr) {
      const r = await send("Runtime.evaluate", {
        expression: expr,
        awaitPromise: true,
        returnByValue: true,
      });
      if (r.exceptionDetails)
        throw new Error(r.exceptionDetails.exception?.description ?? "eval failed");
      return r.result.value;
    },
    async waitFor(expr, timeout = 10000) {
      const t0 = Date.now();
      while (Date.now() - t0 < timeout) {
        if (await api.eval(`Boolean(${expr})`).catch(() => false)) return;
        await sleep(100);
      }
      throw new Error(`timeout waiting for: ${expr}`);
    },
    /** Sets a React-controlled input value. */
    async type(selector, text) {
      await api.eval(`(() => {
        const el = document.querySelector(${JSON.stringify(selector)});
        if (!el) throw new Error("no element ${selector}");
        const proto = el.tagName === "SELECT" ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
        Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ${JSON.stringify(text)});
        el.dispatchEvent(new Event("input", { bubbles: true }));
        el.dispatchEvent(new Event("change", { bubbles: true }));
      })()`);
    },
    /** Clicks the first element in `scope` whose trimmed text equals `text` (falls back to "includes"). */
    async clickText(text, scope = "button") {
      await api.eval(`(() => {
        const all = [...document.querySelectorAll(${JSON.stringify(scope)})];
        const el = all.find((e) => e.textContent.trim() === ${JSON.stringify(text)}) ?? all.find((e) => e.textContent.trim().includes(${JSON.stringify(text)}));
        if (!el) throw new Error("no ${scope} with text: ${text}");
        el.click();
      })()`);
    },
    async click(selector) {
      await api.eval(
        `(() => { const el = document.querySelector(${JSON.stringify(selector)}); if (!el) throw new Error("no ${selector}"); el.click(); })()`,
      );
    },
    async key(key, { ctrl = false } = {}) {
      for (const type of ["keyDown", "keyUp"]) {
        await send("Input.dispatchKeyEvent", {
          type,
          key,
          code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
          modifiers: ctrl ? 2 : 0,
        });
      }
    },
    async shot(path) {
      const r = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(path, Buffer.from(r.data, "base64"));
    },
    sleep,
    close() {
      ws.close();
      proc.kill();
    },
  };
  return api;
}
