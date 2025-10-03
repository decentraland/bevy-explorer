const fs = require("fs");
const dotenv = require("dotenv");

let ENV_CONTENT = {};

if (fs.existsSync(".env")) {
  Object.assign(ENV_CONTENT, dotenv.parse(fs.readFileSync(".env")));
}

const packageJson = JSON.parse(fs.readFileSync("./package.json").toString());

ENV_CONTENT["REACT_APP_WEBSITE_VERSION"] = packageJson.version;

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