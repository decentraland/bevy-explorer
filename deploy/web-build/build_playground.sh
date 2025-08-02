#!/bin/bash

set -e

echo "--- installing playground node dependencies ---"
npm i
echo "--- running playground build ---"
npx vite build --base=/${GITHUB_SHA}/
echo "--- copying playground artifacts ---"
cp -r ./pkg ../web
echo "--- generating node modules for snapshot ---"
mkdir dcl-deps
cd dcl-deps
echo "--- setting snapshot requirements ---"
npm init -y
npm pkg set overrides.esbuild="npm:esbuild-wasm@^0.25.8"
echo "--- installing snapshot requirements ---"
npm i @dcl/sdk@protocol-squad @dcl-sdk/utils @dcl/js-runtime esbuild-wasm
echo "--- resolving symlinks ---"
cp -rL node_modules node_modules_full
cd ..
echo "--- creating snapshot ---"
node create-snapshot.mjs
echo "--- playground build done ---"
