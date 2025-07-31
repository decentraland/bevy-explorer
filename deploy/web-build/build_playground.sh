npm i
npm run build
cp -r dist ../deploy/web
mkdir dcl-deps
cd dcl-deps
npm init -y
npm pkg set overrides.esbuild="npm:esbuild-wasm@^0.25.8"
npm i @dcl/sdk@protocol-squad @dcl-sdk/utils @dcl/js-runtime esbuild-wasm
cd ..
node create-snapshot.mjs
