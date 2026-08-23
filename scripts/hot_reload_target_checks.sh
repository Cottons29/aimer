#!/bin/sh
#
# Compiles every target configuration the hot-reload milestone allows.
#
# The configuration matrix mirrors `aimer_cli/src/commands/run/hot_reload/targets.rs`:
# every family builds both native ahead-of-time profiles, and every native
# family additionally builds the development host with its reload listener. The
# web family is checked only in native ahead-of-time mode, because hot reload is
# rejected for it by CLI policy.
#
# `cargo check` is used deliberately: it type-checks and expands every target's
# `cfg` tree without needing a platform linker, so one host can validate the
# whole matrix with nothing but the corresponding rustup std components.
#
# The development host additionally compiles the reload protocol, whose
# cryptography crate builds C and assembly sources. That configuration therefore
# also needs a C compiler for the selected target: Xcode provides it for Apple
# targets, `ANDROID_NDK_HOME` provides it for Android, and the Windows
# configuration must run on a Windows host with its MSVC toolchain.
#
# Usage:
#   scripts/hot_reload_target_checks.sh [family ...]
#
# Environment:
#   ANDROID_NDK_HOME             Android NDK used for the Android reload host
#   AIMER_REQUIRE_ALL_TARGETS=1  treat a skipped configuration as a failure
#                                (continuous integration sets this)

set -eu

HOST_PACKAGE="aimer_quiver"
RELOAD_FEATURE="wasm-hot-reload"

FAMILIES="macos windows linux ios-simulator ios-device android web"

triple_of() {
    case "$1" in
    macos) echo "aarch64-apple-darwin" ;;
    windows) echo "x86_64-pc-windows-msvc" ;;
    linux) echo "x86_64-unknown-linux-gnu" ;;
    ios-simulator) echo "aarch64-apple-ios-sim" ;;
    ios-device) echo "aarch64-apple-ios" ;;
    android) echo "aarch64-linux-android" ;;
    web) echo "wasm32-unknown-unknown" ;;
    *)
        echo "unknown target family '$1'" >&2
        exit 2
        ;;
    esac
}

supports_hot_reload() {
    [ "$1" != "web" ]
}

# Prints the Android target C compiler and archiver when the NDK is available.
android_c_toolchain() {
    ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
    [ -n "$ndk" ] || return 1
    for directory in "$ndk"/toolchains/llvm/prebuilt/*/bin; do
        [ -d "$directory" ] || continue
        for compiler in "$directory"/aarch64-linux-android*-clang; do
            [ -x "$compiler" ] || continue
            echo "$compiler $directory/llvm-ar"
            return 0
        done
    done
    return 1
}

host_is_windows() {
    case "$(uname -s)" in
    CYGWIN* | MINGW* | MSYS* | Windows*) return 0 ;;
    *) return 1 ;;
    esac
}

installed_targets=$(rustup target list --installed 2>/dev/null | tr '\n' ' ')

failed=""
skipped=""
checked=0

run_check() {
    description="$1"
    shift
    printf '\n==> %s\n' "$description"
    if cargo check "$@"; then
        checked=$((checked + 1))
    else
        failed="$failed\n  - $description"
    fi
}

selected=${*:-$FAMILIES}

for family in $selected; do
    triple=$(triple_of "$family")
    case " $installed_targets " in
    *" $triple "*) ;;
    *)
        skipped="$skipped\n  - $family ($triple): rustup target not installed"
        continue
        ;;
    esac

    run_check "$family native ahead-of-time debug" \
        -p "$HOST_PACKAGE" --target "$triple"
    run_check "$family native ahead-of-time release" \
        -p "$HOST_PACKAGE" --target "$triple" --release
    supports_hot_reload "$family" || continue

    reload_description="$family development host with the reload listener"
    case "$family" in
    android)
        if toolchain=$(android_c_toolchain); then
            CC_aarch64_linux_android=${toolchain%% *}
            AR_aarch64_linux_android=${toolchain##* }
            export CC_aarch64_linux_android AR_aarch64_linux_android
            run_check "$reload_description" \
                -p "$HOST_PACKAGE" --target "$triple" --features "$RELOAD_FEATURE"
            unset CC_aarch64_linux_android AR_aarch64_linux_android
        else
            skipped="$skipped\n  - $reload_description: set ANDROID_NDK_HOME so the reload cryptography can be compiled for Android"
        fi
        ;;
    windows)
        if host_is_windows; then
            run_check "$reload_description" \
                -p "$HOST_PACKAGE" --target "$triple" --features "$RELOAD_FEATURE"
        else
            skipped="$skipped\n  - $reload_description: the reload cryptography needs the MSVC C toolchain, so run this configuration on a Windows host"
        fi
        ;;
    *)
        run_check "$reload_description" \
            -p "$HOST_PACKAGE" --target "$triple" --features "$RELOAD_FEATURE"
        ;;
    esac
done

printf '\n==> CLI hot-reload workflow, socket, recovery, and cleanup proof\n'
cargo test -p aimer_cli --all-features --lib hot_reload -- --test-threads=1

printf '\nchecked %s configurations\n' "$checked"

if [ -n "$skipped" ]; then
    printf 'skipped configurations:%b\n' "$skipped"
    if [ "${AIMER_REQUIRE_ALL_TARGETS:-0}" = "1" ]; then
        printf 'AIMER_REQUIRE_ALL_TARGETS=1 requires every target to be installed\n' >&2
        exit 1
    fi
fi

if [ -n "$failed" ]; then
    printf 'failed configurations:%b\n' "$failed" >&2
    exit 1
fi
