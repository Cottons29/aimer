#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(CDPATH= cd -- "$script_dir/.." && pwd)"
cd "$repo_dir"

# The host tests exercise the selected native dispatch and the forced scalar
# control through the same parity suite. Cross checks below compile the exact
# private kernel for each deployment family; they do not require a device.
cargo test -p aimer_flex --release --lib -- --test-threads=1
cargo test -p aimer_flex --features force-scalar --release --lib -- --test-threads=1

for target in \
    x86_64-apple-darwin \
    aarch64-apple-darwin \
    aarch64-apple-ios \
    aarch64-apple-ios-sim
do
    cargo check -p aimer_flex --release --target "$target"
done

# Android needs an installed NDK/linker in addition to the Rust target. Keep it
# opt-in so desktop CI still validates every source dispatch arm it can build;
# a device/NDK job can enable the additional check explicitly.
if [[ "${SIMD_CHECK_ANDROID:-0}" == "1" ]]; then
    cargo check -p aimer_flex --release --target aarch64-linux-android
fi

# WebAssembly intentionally selects the scalar implementation. Build it both
# without and with the optional wasm SIMD target feature so browsers that
# expose simd128 remain a supported build configuration without becoming a
# requirement for browsers that do not.
cargo check -p aimer_flex --release --target wasm32-unknown-unknown

wasm_rustflags="${RUSTFLAGS:-}"
if [[ -n "$wasm_rustflags" ]]; then
    wasm_rustflags+=" "
fi
wasm_rustflags+="-C target-feature=+simd128"
RUSTFLAGS="$wasm_rustflags" \
    cargo check -p aimer_flex --release --target wasm32-unknown-unknown
