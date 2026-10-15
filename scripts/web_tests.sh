#!/usr/bin/env bash
# Runs the browser tests (wasm-bindgen-test) in headless Chrome.
#
# These cover what native tests cannot: the IndexedDB adapter in a real browser.
#
# One-time setup:
#   cargo install wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock> --locked
#   a chromedriver matching your Chrome's major.minor.build, from
#   https://googlechromelabs.github.io/chrome-for-testing/ , either on PATH or
#   pointed to by CHROMEDRIVER=/path/to/chromedriver
#
# Usage:
#   scripts/web_tests.sh                 # every browser test target
#   scripts/web_tests.sh -p aimer_storage --test indexed_db
#   NO_HEADLESS=1 scripts/web_tests.sh   # watch the browser run
set -euo pipefail
cd "$(dirname "$0")/.."

wanted=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[version = "]/, ""); print; exit}' Cargo.lock)
if ! command -v wasm-bindgen-test-runner >/dev/null; then
    echo "wasm-bindgen-test-runner is not installed." >&2
    echo "  cargo install wasm-bindgen-cli --version ${wanted} --locked" >&2
    exit 1
fi
have=$(wasm-bindgen-test-runner --version | awk '{print $2}')
if [ "${have}" != "${wanted}" ]; then
    # The runner and the wasm-bindgen crate must be the same version exactly.
    echo "wasm-bindgen-test-runner is ${have}, but Cargo.lock pins wasm-bindgen ${wanted}." >&2
    echo "  cargo install wasm-bindgen-cli --version ${wanted} --locked --force" >&2
    exit 1
fi
if [ -z "${CHROMEDRIVER:-}" ] && ! command -v chromedriver >/dev/null; then
    echo "chromedriver was not found: put it on PATH or set CHROMEDRIVER=/path/to/chromedriver." >&2
    exit 1
fi

export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner
if [ "$#" -gt 0 ]; then
    exec cargo test --target wasm32-unknown-unknown "$@"
fi

# Only `#[wasm_bindgen_test]` targets can run in a browser, so the default names
# them rather than every test target (a crate's plain unit tests are native).
cargo test --target wasm32-unknown-unknown -p aimer_storage --test indexed_db
# aimer_persist reaches the widget crates, which need a rendering backend chosen
# for the web; naming aimer_cupid is what lets the feature be enabled for it.
cargo test --target wasm32-unknown-unknown -p aimer_persist -p aimer_cupid \
    --features aimer_cupid/web --test web
