// Playwright driver for the scene-kill browser tests (PR #1091).
//
// Prereqs (all local):
//   python3 serve.py 8111                        (this dir - realm/content server)
//   npm run dev in react-web                     (vite on BASE below)
//   fresh engine build in deploy/web/engine/pkg  (wasm + sandbox_worker.bundle.js)
//
// Runs every mode from game.js sequentially in one Chromium instance, matching the
// engine/worker kill-ladder console lines against per-mode expectations. Assertions are
// scoped to the kill-test scene's numeric id (extracted from its spawn log line): the
// page also runs the bridge scene and the basiccontroller portable, whose own teardowns
// take the leak path by design (parked system-api futures) and must not bleed into the
// mode's pass/fail. Full per-mode console logs land in .work/logs/<mode>.log.
//
//   node run-browser-tests.mjs [--headed] [mode ...]

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "../../react-web/node_modules/playwright/index.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
// react-web's vite dev server; it hops ports when 5173 is taken, so allow an override
const BASE = process.env.KILL_TEST_BASE || "http://localhost:5173";
const REALM = "http://localhost:8111";
const BOOT_TIMEOUT_MS = 240_000;
const LOG_DIR = join(HERE, ".work", "logs");
mkdirSync(LOG_DIR, { recursive: true });

// Per mode: parcel, kill trigger, and expectation builders taking the kill scene's
// numeric id. All engine/worker ladder lines carry the id; the watchdog line carries
// the scene title (kill-<mode>).
const MODES = {
  graceful: {
    parcel: "0,0",
    kill: "reload",
    expect: (id) => [
      new RegExp(`kill requested for scene ${id}; awaiting graceful exit`),
      new RegExp(`scene ${id}: teardown complete`),
      new RegExp(`scene ${id} worker exited cleanly`),
    ],
    forbid: (id) => [
      new RegExp(`scene ${id} still running after kill`),
      new RegExp(`scene ${id}: \\d+ op future`),
      new RegExp(`scene ${id}: scene state still in use`),
      new RegExp(`scene ${id} forcibly terminated`),
    ],
  },
  asyncwedge: {
    parcel: "40,0",
    kill: "wedge",
    expect: (id) => [
      /kill-asyncwedge @ .*has not responded for .*marking broken/,
      new RegExp(`kill requested for scene ${id}; awaiting graceful exit`),
      new RegExp(`scene ${id} still running after kill; posting SHUTDOWN`),
      new RegExp(`scene ${id}: teardown complete`),
      new RegExp(`scene ${id} worker exited cleanly`),
    ],
    forbid: (id) => [
      new RegExp(`scene ${id}: \\d+ op future`),
      new RegExp(`scene ${id}: scene state still in use`),
      new RegExp(`scene ${id} forcibly terminated`),
    ],
  },
  hangop: {
    parcel: "80,0",
    kill: "wedge",
    expect: (id) => [
      /kill-hangop @ .*has not responded for .*marking broken/,
      new RegExp(`scene ${id} still running after kill; posting SHUTDOWN`),
      new RegExp(`scene ${id}: \\d+ op future\\(s\\) still hold scene state`),
      new RegExp(`scene ${id}: scene state still in use; leaking thread state`),
      new RegExp(`scene ${id}: teardown complete`),
      new RegExp(`scene ${id} worker exited cleanly`),
    ],
    forbid: (id) => [new RegExp(`scene ${id} forcibly terminated`)],
  },
  spin: {
    parcel: "120,0",
    kill: "wedge",
    expect: (id) => [
      /kill-spin @ .*has not responded for .*marking broken/,
      new RegExp(`scene ${id} still running after kill; posting SHUTDOWN`),
      new RegExp(`scene ${id} did not respond to SHUTDOWN \\(sync spin\\?\\); force-terminating`),
      new RegExp(`scene ${id} forcibly terminated; thread state leaked`),
    ],
    forbid: (id) => [new RegExp(`scene ${id}: teardown complete`)],
  },
  opspin: {
    parcel: "160,0",
    kill: "wedge",
    expect: (id) => [
      /kill-opspin @ .*has not responded for .*marking broken/,
      new RegExp(`scene ${id} still running after kill; posting SHUTDOWN`),
      new RegExp(`scene ${id} did not respond to SHUTDOWN \\(sync spin\\?\\); force-terminating`),
      new RegExp(`scene ${id}: kill flag set; parking until terminate`),
      new RegExp(`scene ${id} forcibly terminated; thread state leaked`),
    ],
    forbid: (id) => [new RegExp(`scene ${id}: teardown complete`)],
  },
  forge: {
    parcel: "200,0",
    kill: "reload",
    expect: (id) => [
      /dropped untokened message from a sandbox worker/,
      /dropped SHUTDOWN without valid token/,
      /dropped duplicate INIT_WORKER/,
      /KILLTEST\|forge\|forged\|/, // scene survived its own forgeries...
      new RegExp(`kill requested for scene ${id}; awaiting graceful exit`), // ...and a real kill still works
      new RegExp(`scene ${id}: teardown complete`),
      new RegExp(`scene ${id} worker exited cleanly`),
    ],
    forbid: (id) => [new RegExp(`scene ${id} forcibly terminated`)],
  },
};

// Engine-level red flags in any mode: wasm corruption / the old silent drop_context bug.
const GLOBAL_FORBID = [
  /null pointer passed to rust/,
  /error dropping scene context/,
  /RuntimeError: unreachable/,
  /blocked inside engine wasm/,
];

const args = process.argv.slice(2);
const headed = args.includes("--headed");
const only = args.filter((a) => !a.startsWith("--"));
const modes = only.length ? only : Object.keys(MODES);

function makeWaiter(lines) {
  const pending = [];
  return {
    push(line) {
      lines.push(line);
      for (let i = pending.length - 1; i >= 0; i--) {
        if (pending[i].re.test(line)) {
          pending[i].resolve(line);
          pending.splice(i, 1);
        }
      }
    },
    waitFor(re, timeoutMs, label) {
      const hit = lines.find((l) => re.test(l));
      if (hit) return Promise.resolve(hit);
      return new Promise((resolve, reject) => {
        const t = setTimeout(
          () => reject(new Error(`timeout waiting for ${label || re}`)),
          timeoutMs
        );
        pending.push({ re, resolve: (l) => { clearTimeout(t); resolve(l); } });
      });
    },
  };
}

const browser = await chromium.launch({
  headless: !headed,
  args: ["--enable-unsafe-webgpu"],
});
const context = await browser.newContext();
const results = [];

for (const mode of modes) {
  const spec = MODES[mode];
  if (!spec) { console.error(`unknown mode ${mode}`); continue; }
  console.log(`\n=== ${mode} (parcel ${spec.parcel}) ===`);
  const lines = [];
  const waiter = makeWaiter(lines);
  const page = await context.newPage();
  page.on("console", (msg) => waiter.push(msg.text()));
  page.on("pageerror", (err) => waiter.push(`PAGEERROR: ${err.message}`));

  const failures = [];
  try {
    await page.goto(`${BASE}/?realm=${encodeURIComponent(REALM)}&position=${spec.parcel}&guest=1&hud=0`);
    // the kill scene's numeric id, from its spawn log line — every ladder assertion is
    // scoped to it (the bridge scene / portable tear down on their own terms)
    const spawnRe = new RegExp(`spawning scene "bafkkillscene[0-9a-f]+${mode}" @ [^:]*: [0-9v]+#(\\d+)`);
    const spawnLine = await waiter.waitFor(spawnRe, BOOT_TIMEOUT_MS, "kill scene spawn");
    const sceneId = spawnLine.match(spawnRe)[1];
    await waiter.waitFor(new RegExp(`KILLTEST\\|${mode}\\|scene-start`), BOOT_TIMEOUT_MS, "scene-start");
    console.log(`  scene up (id ${sceneId})`);

    if (spec.kill === "wedge") {
      await waiter.waitFor(new RegExp(`KILLTEST\\|${mode}\\|wedging`), 60_000, "wedging");
      console.log("  wedged; waiting for the kill ladder (~25s)");
    } else {
      // let it run a few seconds (forge posts its forgeries at tick 30)
      await waiter.waitFor(new RegExp(`KILLTEST\\|${mode}\\|alive`), 30_000, "alive");
      if (mode === "forge") {
        await waiter.waitFor(/KILLTEST\|forge\|forged\|/, 30_000, "forged");
        await page.waitForTimeout(1000);
      }
      console.log("  triggering kill via /reload");
      await page.evaluate(() => window.engine_console_command("/reload"));
    }

    const expect = spec.expect(sceneId);
    const forbid = spec.forbid(sceneId);
    // terminal line per kill style, then let stragglers land — outlast the 5s escalation
    // grace so a lost ack would surface as "still running after kill" and fail the forbids
    const terminal = expect[expect.length - 1];
    await waiter.waitFor(terminal, 45_000, `terminal ${terminal}`);
    await page.waitForTimeout(6500);

    // engine must still be alive and responsive after every kill
    const pos = await page.evaluate(() => window.engine_console_command("/player_position"));
    console.log(`  engine responsive after kill: ${String(pos).trim()}`);

    for (const re of expect) if (!lines.some((l) => re.test(l))) failures.push(`missing: ${re}`);
    for (const re of forbid) { const h = lines.find((l) => re.test(l)); if (h) failures.push(`forbidden: ${re} -> ${h}`); }
    for (const re of GLOBAL_FORBID) { const h = lines.find((l) => re.test(l)); if (h) failures.push(`global forbidden: ${re} -> ${h}`); }
  } catch (e) {
    failures.push(String(e));
  }

  writeFileSync(join(LOG_DIR, `${mode}.log`), lines.join("\n"));
  results.push({ mode, failures });
  console.log(failures.length ? `  FAIL\n    ${failures.join("\n    ")}` : "  PASS");
  await page.close();
}

await browser.close();
console.log("\n=== summary ===");
for (const r of results) console.log(`${r.failures.length ? "FAIL" : "PASS"}  ${r.mode}`);
process.exit(results.some((r) => r.failures.length) ? 1 : 0);
