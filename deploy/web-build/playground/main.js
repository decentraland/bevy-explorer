import { WebContainer } from "@webcontainer/api";
import { initEngine, startEngine } from "/engine/engine.js";
import path from "path-browserify";

// Get DOM elements
const buildButton = document.getElementById("buildButton");
const sourceCodeEl = document.getElementById("sourceCode");
const outputEl = document.getElementById("output");

let webcontainerInstance;

// --- Main Logic ---
window.addEventListener("load", async () => {
  outputEl.textContent = "Booting WebContainer...\n";
  webcontainerInstance = await WebContainer.boot();
  webcontainerInstance.on("error", (error) => {
    console.error("A WebContainer error occurred:", error);
    outputEl.textContent += `\n\nFATAL WEBCONTAINER ERROR: ${error.message}`;
  });
  outputEl.textContent += "WebContainer booted. Loading scene files...\n";
  try {
    const sceneFiles = ["package.json", "scene.json", "tsconfig.json"];
    for (const filePath of sceneFiles) {
      const response = await fetch(`../scene-fs/${filePath}`);
      if (!response.ok) throw new Error(`Fetch failed for ${filePath}`);
      const content = await response.text();
      await webcontainerInstance.fs.writeFile(`/${filePath}`, content);
    }
    outputEl.textContent += "Scene config files loaded.\n";
    // Fetch the official snapshot file and mount it
    outputEl.textContent += "Loading dependencies snapshot...\n";
    const response = await fetch("../assets/node_modules.snapshot");
    if (!response.ok)
      throw new Error(
        "Fetch failed for /node_modules.snapshot. Make sure it's in the 'public' directory."
      );
    const snapshotBuffer = await response.arrayBuffer();
    const snapshotUint8Array = new Uint8Array(snapshotBuffer);
    outputEl.textContent += "Mounting dependencies snapshot...\n";
    await webcontainerInstance.fs.mkdir("/node_modules", { recursive: true });
    await webcontainerInstance.mount(snapshotUint8Array, {
      mountPoint: "/node_modules",
    });
    outputEl.textContent += "Dependencies mounted. Ready to build.\n";
  } catch (error) {
    console.error("Error during setup:", error);
    outputEl.textContent += `\nError during setup: ${error.message}\n`;
  }
});

// --- Build Logic ---
buildButton.addEventListener("click", async () => {
  outputEl.textContent = "";
  const sourceCode = sourceCodeEl.value;
  if (!webcontainerInstance || !sourceCode) {
    outputEl.textContent =
      "Error: WebContainer not ready or no source code provided.\n";
    return;
  }

  try {
    // Write the user's source code
    outputEl.textContent = "Writing source file...\n";
    await webcontainerInstance.fs.mkdir("/src", { recursive: true });
    await webcontainerInstance.fs.writeFile("/src/index.ts", sourceCode);

    outputEl.textContent += "Running build...\n";
    const buildProcess = await webcontainerInstance.spawn(
      "node",
      ["node_modules/@dcl/sdk-commands/dist/index.js", "build"],
      {
        env: { CI: "true" },
      }
    );

    outputEl.textContent += "Setting script permissions (chmod)...\n";
    const chmodProcess1 = await webcontainerInstance.spawn("chmod", [
      "-R",
      "+x",
      "./node_modules/.bin",
    ]);
    await chmodProcess1.exit;

    const chmodProcess2 = await webcontainerInstance.spawn("chmod", [
      "-R",
      "+x",
      "./node_modules/@esbuild/",
    ]);
    await chmodProcess2.exit;

    // Use the verified output streaming method
    buildProcess.output.pipeTo(
      new WritableStream({
        write(data) {
          function stripAnsiCodes(str) {
            return str.replace(/\x1B\[[0-?]*[ -/]*[@-~]/g, "");
          }
          outputEl.textContent += stripAnsiCodes(data);
        },
      })
    );

    const buildExitCode = await buildProcess.exit;

    if (buildExitCode !== 0) {
      outputEl.textContent += `\n\nBuild failed with exit code ${buildExitCode}.\n`;
      return;
    }

    outputEl.textContent += `--- BUILD SUCCEEDED ---\n\n`;

    const game_js = await webcontainerInstance.fs.readFile(
      "/bin/game.js",
      "utf-8"
    );

    const game_js_blob = new Blob([game_js], {
      type: "application/javascript",
    });
    const content = new Map([["game.js", URL.createObjectURL(game_js_blob)]]);

    const meta = await sceneMeta(content);
    content.set("bevyPlaygroundHash", meta);

    // start explorer
    await initEngine();

    const originalFetch = window.fetch;
    window.fetch = async (input, init) => {
      let url;

      if (typeof input === "string") {
        url = input;
      } else if (input instanceof Request) {
        url = input.url;
      } else {
        return originalFetch.apply(this, arguments);
      }

      if (url.startsWith("https://redirect/")) {
        const redirect_url = url.substring("https://redirect/".length);
        if (content.has(redirect_url)) {
          const newUrl = content.get(redirect_url);
          console.log(`Redirecting fetch: ${url} -> ${newUrl}`);

          // Replace the input with the new URL
          if (typeof input === "string") {
            input = newUrl;
          } else {
            // Re-create the Request object with the new URL
            input = new Request(newUrl, input);
          }
        } else {
          // 4. Call the original fetch with the (potentially modified) arguments
          console.log(`NOT redirecting fetch: ${url}`);
        }
      }
      return originalFetch.apply(this, [input, init]);
    };

    let realm = String(new URL("/realm-provider", window.location.origin));
    startEngine(realm, "0,0", "");
  } catch (e) {
    outputEl.textContent += `\n\nAn error occurred during the build process. Error: ${e.message}\n`;
  }
});

async function sceneMeta(content) {
  const meta_fetch = await fetch(
    new URL("/realm-provider/scene_meta", window.location.origin)
  );
  var meta_content = await meta_fetch.text();
  var meta_json = JSON.parse(meta_content);
  for (const [f, h] of content) {
    meta_json["content"].push({ file: f, hash: f });
  }
  const meta_str = JSON.stringify(meta_json);
  console.log(`meta: ${meta_str}`);
  const blob = new Blob([meta_str], { type: "application/json" });
  return URL.createObjectURL(blob);
}
