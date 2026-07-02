const fs = require("fs");

let ENV_CONTENT = {};

const packageJson = JSON.parse(fs.readFileSync("./package.json").toString());

Object.assign(ENV_CONTENT, getPublicUrls());

packageJson.homepage = ENV_CONTENT["PUBLIC_URL"];

if (packageJson.homepage) {
  // github action outputs. Do not touch.
  console.log("::set-output name=public_url::" + packageJson.homepage);
  console.log(
    "::set-output name=public_path::" + new URL(packageJson.homepage).pathname
  );
}

console.log("VERSIONS: ", Object.entries(ENV_CONTENT), "\n");

fs.writeFileSync(
  ".env",
  Object.entries(ENV_CONTENT)
    .map((e) => e[0] + "=" + JSON.stringify(e[1]))
    .join("\n") + "\n"
);

fs.writeFileSync("./package.json", JSON.stringify(packageJson, null, 2));

// No HTML rewriting: the React app (the only page) bakes PUBLIC_URL in at build time (vite base),
// and sets window.PUBLIC_URL for the engine's pkg/ fetches (react-web EngineHost). This script's
// remaining job is stamping homepage/.env and the ::set-output lines the CDN deployer reads.

function getPublicUrls() {
  if (!process.env.GEN_STATIC_LOCAL) {
    if (process.env.CI) {
      return {
        PUBLIC_URL: `https://cdn.decentraland.org/${packageJson.name}/${packageJson.version}`,
      };
    }
  }

  return {
    PUBLIC_URL: ``,
  };
}