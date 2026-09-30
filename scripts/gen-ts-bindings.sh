#!/usr/bin/env sh
# Generate the TypeScript bindings for the ~system/BevyExplorerApi boundary from the Rust
# structs in crates/system_api_types, into react-web/src/engine/generated (+ a barrel index.ts).
# Also writes the web param TABLE (crates/system_api_types/src/web_params.rs → webParamTable.ts):
# the `export_` filter catches both ts-rs's export_bindings_* tests and export_web_param_table.
# Called by `just ts-bindings` and by CI (ci.yml deploy-web, package.yml) before the
# react-web / bridge-scene builds, which import the generated (gitignored) types.
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
# git-bash (windows CI): cargo is a native process, so hand it a windows-style path —
# the POSIX /d/... form would be misresolved.
case "$(uname -s)" in
MINGW* | MSYS*) root="$(cd "$root" && pwd -W)" ;;
esac
out="$root/react-web/src/engine/generated"
rm -rf "$out"
mkdir -p "$out"
TS_RS_EXPORT_DIR="$out" cargo test --manifest-path "$root/Cargo.toml" -p system_api_types export_
cd "$out"
: >index.ts
for f in $(ls ./*.ts 2>/dev/null | grep -v '^\./index\.ts$' | LC_ALL=C sort); do
    f="${f#./}"
    echo "export * from './${f%.ts}'" >>index.ts
done
