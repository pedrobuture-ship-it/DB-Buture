// End to end smoke run of the real UI against real databases.
//   node scripts/e2e/run.mjs <outDir>
// Needs: dev bridge (cargo build --example dev_bridge), the Vite dev server, and the Docker
// test databases (see src-tauri/tests). Writes screenshots to <outDir>.
import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { launchBrowser } from "./cdp.mjs";

const out = resolve(process.argv[2] ?? "e2e-out");
mkdirSync(out, { recursive: true });
const root = resolve(import.meta.dirname, "../..");
const children = [];
const start = (cmd, args, opts = {}) => {
  const p = spawn(cmd, args, { stdio: "ignore", cwd: root, ...opts });
  children.push(p);
  return p;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

start(join(root, "src-tauri/target/debug/examples/dev_bridge"), []);
start("npx", ["vite", "--port", "1420", "--strictPort"]);
await sleep(2500);

const b = await launchBrowser();
let failed = 0;
const step = async (name, fn) => {
  try {
    await fn();
    console.log(`ok   ${name}`);
  } catch (e) {
    failed++;
    console.log(`FAIL ${name}: ${e.message}`);
    await b.shot(join(out, `FAIL-${name.replace(/\W+/g, "-")}.png`)).catch(() => {});
  }
};

try {
  await b.inject(join(root, "scripts/e2e/shim.js"));
  await b.goto("http://localhost:1420");

  await step("connect screen", async () => {
    await b.waitFor(`document.body.innerText.includes("CONNECT DATABASE")`);
    await b.shot(join(out, "01-connect.png"));
  });

  await step("postgres test connection", async () => {
    const inputs = await b.eval(
      `[...document.querySelectorAll("input")].map(i => i.placeholder + "|" + i.type)`,
    );
    // fields are in DOM order: name, host, port, database, username, password
    await b.eval(`(() => {
      const set = (el, v) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, v); el.dispatchEvent(new Event("input", { bubbles: true })); };
      const i = [...document.querySelectorAll("form input")];
      set(i[0], "test pg"); set(i[1], "127.0.0.1"); set(i[2], "54329"); set(i[3], "buture"); set(i[4], "postgres"); set(i[5], "buture_test");
      const sel = document.querySelector("form select");
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(sel, "disable");
      sel.dispatchEvent(new Event("change", { bubbles: true }));
    })()`);
    await b.clickText("Test Connection");
    await b.waitFor(`document.body.innerText.includes("CONNECTION SUCCESSFUL")`);
    const pw = await b.eval(`document.querySelector('input[type=password]').value`);
    if (pw !== "") throw new Error("password was not cleared after submit");
    await b.shot(join(out, "02-test-ok.png"));
    void inputs;
  });

  await step("postgres connect", async () => {
    await b.eval(`(() => {
      const el = document.querySelector('input[type=password]');
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(el, "buture_test");
      el.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
    await b.clickText("Connect");
    await b.waitFor(`document.body.innerText.includes("DATABASE OVERVIEW")`);
    await b.sleep(400);
    await b.shot(join(out, "03-overview.png"));
  });

  await step("explorer", async () => {
    await b.clickText("Explorer", "nav button");
    await b.waitFor(`document.querySelector('[role=tree]')`);
    await b.eval(
      `(() => { const t = [...document.querySelectorAll('[role=treeitem]')].find(e => e.textContent.includes("shop")); t && t.click(); })()`,
    );
    await b.sleep(300);
    await b.eval(
      `(() => { const t = [...document.querySelectorAll('[role=treeitem]')].find(e => e.textContent.trim().startsWith("orders")); t && t.click(); })()`,
    );
    await b.waitFor(
      `document.body.innerText.includes("orders_user_fk") || document.body.innerText.includes("user_id")`,
    );
    await b.shot(join(out, "04-explorer.png"));
  });

  await step("er diagram", async () => {
    await b.clickText("ER Diagram", "nav button");
    await b.waitFor(`document.querySelectorAll('.react-flow__node').length > 3`, 15000);
    await b.sleep(900);
    await b.shot(join(out, "05-erd.png"));
  });

  await step("dependencies", async () => {
    await b.clickText("Dependencies", "nav button");
    await b.waitFor(`document.querySelectorAll('.react-flow__node').length > 3`, 15000);
    await b.sleep(900);
    await b.shot(join(out, "06-deps.png"));
  });

  await step("impact", async () => {
    await b.clickText("Explorer", "nav button");
    await b.eval(
      `(() => { const t = [...document.querySelectorAll('[role=treeitem]')].find(e => e.textContent.trim().startsWith("users")); t && t.click(); })()`,
    );
    await b.waitFor(
      `[...document.querySelectorAll("button")].some(b => b.textContent.trim() === "Impact")`,
    );
    await b.clickText("Impact");
    await b.waitFor(
      `document.body.innerText.includes("POTENTIAL IMPACT") && document.body.innerText.includes("TRANSITIVE")`,
    );
    await b.sleep(900);
    await b.shot(join(out, "07-impact.png"));
  });

  await step("health", async () => {
    await b.clickText("Health", "nav button");
    await b.waitFor(
      `document.body.innerText.includes("DATABASE HEALTH") && document.body.innerText.includes("out of 100")`,
    );
    await b.shot(join(out, "08-health.png"));
  });

  await step("index analysis", async () => {
    await b.clickText("Index Analysis", "nav button");
    await b.waitFor(`document.body.innerText.includes("COPY SQL")`);
    await b.shot(join(out, "09-index-analysis.png"));
  });

  await step("command palette", async () => {
    await b.key("k", { ctrl: true });
    await b.waitFor(`document.querySelector('[role=dialog]')`);
    await b.type("[role=dialog] input", "email");
    await b.sleep(300);
    await b.shot(join(out, "10-palette.png"));
    await b.key("Escape");
  });

  console.log(b.logs.length ? `browser logs:\n${b.logs.join("\n")}` : "no browser errors");
} finally {
  b.close();
  for (const c of children) c.kill();
}
process.exit(failed ? 1 : 0);
