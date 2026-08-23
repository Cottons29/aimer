#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

apply_file() {
    source=$1
    target=$2
    temporary="$(dirname -- "$target")/.$(basename -- "$target").tmp"
    cp -- "$source" "$temporary"
    mv -- "$temporary" "$target"
}

variant_source() {
    case "$1" in
        initial) echo "$ROOT/variants/01_initial.rs" ;;
        widget-body) echo "$ROOT/variants/02_widget_body.rs" ;;
        schema-migration) echo "$ROOT/variants/03_schema_migration.rs" ;;
        callback-rebind) echo "$ROOT/variants/04_callback_rebind.rs" ;;
        compile-failure) echo "$ROOT/variants/05_compile_failure.rs" ;;
        initial-build-trap) echo "$ROOT/variants/06_initial_build_trap.rs" ;;
        recovery) echo "$ROOT/variants/07_recovery.rs" ;;
        native-marker) echo "$ROOT/variants/08_native_marker.contract" ;;
        *) return 1 ;;
    esac
}

apply_variant() {
    source=$(variant_source "$1") || {
        printf '%s\n' "unknown variant: $1" >&2
        exit 2
    }
    if [ "$1" = native-marker ]; then
        apply_file "$source" "$ROOT/native/contract.marker"
    else
        apply_file "$source" "$ROOT/src/guest.rs"
    fi
}

check_variants() {
    saved=$(mktemp "${TMPDIR:-/tmp}/aimer-full-state-guest.XXXXXX")
    cp -- "$ROOT/src/guest.rs" "$saved"
    trap 'apply_file "$saved" "$ROOT/src/guest.rs"; rm -f -- "$saved"' EXIT HUP INT TERM
    for variant in initial widget-body schema-migration callback-rebind initial-build-trap recovery; do
        apply_variant "$variant"
        cargo check --quiet --manifest-path "$ROOT/Cargo.toml" --lib --target wasm32-unknown-unknown
        printf '%s\n' "checked $variant"
    done
    apply_file "$saved" "$ROOT/src/guest.rs"
    rm -f -- "$saved"
    trap - EXIT HUP INT TERM
}

case "${1:-}" in
    apply)
        [ "$#" -eq 2 ] || { printf '%s\n' "usage: $0 apply VARIANT" >&2; exit 2; }
        apply_variant "$2"
        ;;
    check)
        [ "$#" -eq 1 ] || { printf '%s\n' "usage: $0 check" >&2; exit 2; }
        check_variants
        ;;
    *)
        printf '%s\n' "usage: $0 {apply VARIANT|check}" >&2
        exit 2
        ;;
esac
