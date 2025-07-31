npm i
npm run build
cp -r dist ../web
mkdir dcl-deps
cd dcl-deps
npm init -y
npm pkg set overrides.esbuild="npm:esbuild-wasm@^0.25.8"
npm i @dcl/sdk@protocol-squad @dcl-sdk/utils @dcl/js-runtime esbuild-wasm
cp -rL node_modules node_modules_full
cd ..
node create-snapshot.mjs
