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

// Update HTML file to include PUBLIC_URL in script src
const htmlPath = "index.html";
if (fs.existsSync(htmlPath)) {
  let htmlContent = fs.readFileSync(htmlPath).toString();
  const publicUrl = ENV_CONTENT["PUBLIC_URL"];
  const scriptPath = publicUrl ? `${publicUrl}/main.js` : "main.js";

  // Replace the main.js script src
  htmlContent = htmlContent.replace(
    /<script type="module" src="main\.js"><\/script>/,
    `<script type="module" src="${scriptPath}"></script>`
  );

  fs.writeFileSync(htmlPath, htmlContent);
  console.log(`Updated ${htmlPath} with script path: ${scriptPath}`);
}

function getPublicUrls() {
  console.log('Get public urls')
  if (!process.env.GEN_STATIC_LOCAL) {
    console.log('Get public urls 1')
    if (process.env.CI) {
      console.log('Get public urls 2', `https://cdn.decentraland.org/${packageJson.name}/${packageJson.version}`)
      return {
        PUBLIC_URL: `https://cdn.decentraland.org/${packageJson.name}/${packageJson.version}`,
      };
    }
  }

  return {
    PUBLIC_URL: ``,
  };
}