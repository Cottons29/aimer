#!/bin/sh

set -eu

fail() {
    printf 'release audit failed: %s\n' "$1" >&2
    return 1
}

assert_dependency_tree_is_release_safe() {
    tree_file=$1
    for package in wasmi aimer_reload_protocol aimer_reload_server; do
        if awk -v package="$package" '$1 == package { found = 1 } END { exit !found }' "$tree_file"; then
            fail "development dependency '$package' is present in the native AOT graph"
            return 1
        fi
    done
}

assert_artifact_is_release_safe() {
    artifact_file=$1
    strings_file=$2
    if ! command -v strings >/dev/null 2>&1; then
        fail "'strings' is required to inspect release artifacts"
        return 1
    fi
    if ! LC_ALL=C strings "$artifact_file" > "$strings_file" 2>/dev/null; then
        fail "cannot extract strings from '$artifact_file'"
        return 1
    fi
    for marker in \
        AMRH \
        AMRL \
        AIMER_RELOAD_LISTENER_READY \
        AIMER_RELOAD_TOKEN \
        AIMER_RELOAD_SESSION \
        aimer_reload_server \
        reload_command_bridge
    do
        if awk -v marker="$marker" 'index($0, marker) { found = 1 } END { exit !found }' "$strings_file"; then
            fail "development marker '$marker' is present in '$artifact_file'"
            return 1
        fi
    done
}

assert_symbols_are_release_safe() {
    artifact_file=$1
    symbols_file=$2
    if ! command -v nm >/dev/null 2>&1; then
        fail "'nm' is required to inspect release symbols"
        return 1
    fi
    if ! nm "$artifact_file" > "$symbols_file" 2>/dev/null; then
        fail "cannot inspect symbols in '$artifact_file'"
        return 1
    fi
    for marker in wasmi aimer_reload_protocol aimer_reload_server reload_command_bridge; do
        if awk -v marker="$marker" 'index($0, marker) { found = 1 } END { exit !found }' "$symbols_file"; then
            fail "development symbol '$marker' is present in '$artifact_file'"
            return 1
        fi
    done
}

assert_package_is_release_safe() {
    package_dir=$1
    package_files=$2
    if [ ! -d "$package_dir" ]; then
        fail "release package directory '$package_dir' does not exist"
        return 1
    fi
    find "$package_dir" -type f > "$package_files"
    if awk '
        /\.wasm$/ || /\.wat$/ || /reload[_-](token|session|module)/ {
            print
            found = 1
        }
        END { exit !found }
    ' "$package_files" >&2; then
        fail "release package contains a mutable module or reload secret file"
        return 1
    fi

    while IFS= read -r plist; do
        [ -n "$plist" ] || continue
        plist_strings="$package_files.plist-strings"
        if ! LC_ALL=C strings "$plist" > "$plist_strings" 2>/dev/null; then
            fail "cannot inspect packaged plist '$plist'"
            return 1
        fi
        for marker in \
            NSLocalNetworkUsageDescription \
            NSBonjourServices \
            com.apple.developer.networking.multicast \
            com.apple.security.network.server
        do
            if awk -v marker="$marker" 'index($0, marker) { found = 1 } END { exit !found }' "$plist_strings"; then
                fail "development network permission '$marker' is present in '$plist'"
                return 1
            fi
        done
    done <<EOF
$(find "$package_dir" -type f -name 'Info.plist')
EOF
}

assert_entitlements_are_release_safe() {
    artifact_file=$1
    entitlements_file=$2
    codesign_error_file=$3
    if [ ! -r "$artifact_file" ]; then
        fail "release artifact '$artifact_file' is not readable for entitlement inspection"
        return 1
    fi
    if [ "$(uname -s)" != Darwin ]; then
        if ! command -v file >/dev/null 2>&1; then
            fail "'file' is required to determine whether entitlement inspection applies"
            return 1
        fi
        artifact_kind=$(file "$artifact_file") || {
            fail "cannot determine the release artifact format for entitlement inspection"
            return 1
        }
        case "$artifact_kind" in
            *Mach-O*)
                fail "a Mach-O release artifact requires codesign entitlement inspection on macOS"
                return 1
                ;;
        esac
        return 0
    fi
    if ! command -v codesign >/dev/null 2>&1; then
        fail "'codesign' is required to inspect Apple release entitlements"
        return 1
    fi
    if ! codesign -d --entitlements :- "$artifact_file" > "$entitlements_file" 2> "$codesign_error_file"; then
        if awk 'index($0, "code object is not signed at all") { found = 1 } END { exit !found }' "$codesign_error_file"; then
            return 0
        fi
        fail "cannot inspect entitlements in '$artifact_file'"
        return 1
    fi
    for marker in \
        com.apple.developer.networking.multicast \
        com.apple.security.network.server
    do
        if awk -v marker="$marker" 'index($0, marker) { found = 1 } END { exit !found }' "$entitlements_file"; then
            fail "development network entitlement '$marker' is present in '$artifact_file'"
            return 1
        fi
    done
}

assert_release_feature_guard() {
    log_file=$1
    if cargo check -p aimer_quiver --release --features wasm-hot-reload > "$log_file" 2>&1; then
        fail "aimer_quiver accepted wasm-hot-reload in a release build"
        return 1
    fi
    if ! awk '
        index($0, "wasm-hot-reload host feature is available only in debug builds") {
            found = 1
        }
        END { exit !found }
    ' "$log_file"; then
        fail "release feature check failed for a reason other than the compile-time guard"
        return 1
    fi
}

run_audit() {
    script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
    repository=$(CDPATH= cd -- "$script_dir/.." && pwd)
    target=
    artifact=
    package_dir=

    while [ "$#" -gt 0 ]; do
        case "$1" in
            --target)
                target=${2:?missing target triple}
                shift 2
                ;;
            --artifact)
                artifact=${2:?missing artifact path}
                shift 2
                ;;
            --package)
                package_dir=${2:?missing package directory}
                shift 2
                ;;
            *)
                fail "unknown argument '$1'"
                exit 2
                ;;
        esac
    done

    cd "$repository"
    audit_dir=target/hot_reload_release_audit
    tree_file="$audit_dir/dependency-tree.txt"
    symbols_file="$audit_dir/symbols.txt"
    strings_file="$audit_dir/strings.txt"
    package_files="$audit_dir/package-files.txt"
    entitlements_file="$audit_dir/entitlements.plist"
    codesign_error_file="$audit_dir/codesign-error.txt"
    guard_log="$audit_dir/release-feature-guard.log"
    mkdir -p "$audit_dir"

    printf '%s\n' '==> checking the resolved native AOT dependency graph'
    if [ -n "$target" ]; then
        cargo tree -p aimer --edges normal --prefix none --target "$target" > "$tree_file"
    else
        cargo tree -p aimer --edges normal --prefix none > "$tree_file"
    fi
    assert_dependency_tree_is_release_safe "$tree_file"

    if [ -z "$artifact" ]; then
        printf '%s\n' '==> building the native text-field release artifact'
        if [ -n "$target" ]; then
            cargo build --release --example text_field --target "$target"
            artifact="target/$target/release/examples/text_field"
        else
            cargo build --release --example text_field
            artifact=target/release/examples/text_field
        fi
        case "$target" in
            *windows*) artifact="$artifact.exe" ;;
        esac
    fi
    if [ ! -f "$artifact" ]; then
        fail "release artifact '$artifact' does not exist"
        exit 1
    fi

    printf '%s\n' '==> inspecting protocol strings and release symbols'
    assert_artifact_is_release_safe "$artifact" "$strings_file"
    assert_symbols_are_release_safe "$artifact" "$symbols_file"
    assert_entitlements_are_release_safe "$artifact" "$entitlements_file" "$codesign_error_file"

    if [ -n "$package_dir" ]; then
        printf '%s\n' '==> inspecting packaged files and development permissions'
        assert_package_is_release_safe "$package_dir" "$package_files"
    fi

    printf '%s\n' '==> proving the development feature cannot compile in release mode'
    assert_release_feature_guard "$guard_log"
    printf '%s\n' "hot-reload release audit passed for $artifact"
}

self_test() {
    fixture_dir="target/hot_reload_release_audit_self_test"
    safe_tree="$fixture_dir/safe-tree.txt"
    leaked_tree="$fixture_dir/leaked-tree.txt"
    safe_artifact="$fixture_dir/safe-artifact"
    leaked_artifact="$fixture_dir/leaked-artifact"
    strings_file="$fixture_dir/strings.txt"
    symbols_file="$fixture_dir/symbols.txt"
    entitlements_file="$fixture_dir/entitlements.plist"
    codesign_error_file="$fixture_dir/codesign-error.txt"
    trap 'rm -rf "$fixture_dir"' EXIT HUP INT TERM
    mkdir -p "$fixture_dir"

    printf '%s\n' 'aimer v0.1.0' 'aimer_anteros v0.1.0' > "$safe_tree"
    printf '%s\n' 'aimer v0.1.0' 'wasmi v1.1.0' > "$leaked_tree"
    printf '%s\n' 'ordinary native application' > "$safe_artifact"
    printf '%s\n' 'ordinary native application AMRH AMRL' > "$leaked_artifact"

    assert_dependency_tree_is_release_safe "$safe_tree"
    if assert_dependency_tree_is_release_safe "$leaked_tree"; then
        printf '%s\n' 'self-test failed: leaked dependency graph was accepted' >&2
        exit 1
    fi
    assert_artifact_is_release_safe "$safe_artifact" "$strings_file"
    if assert_artifact_is_release_safe "$leaked_artifact" "$strings_file"; then
        printf '%s\n' 'self-test failed: protocol signatures were accepted' >&2
        exit 1
    fi
    if assert_artifact_is_release_safe "$fixture_dir/missing" "$strings_file"; then
        printf '%s\n' 'self-test failed: unreadable artifact passed string inspection' >&2
        exit 1
    fi
    if assert_symbols_are_release_safe "$fixture_dir/missing" "$symbols_file"; then
        printf '%s\n' 'self-test failed: unreadable artifact passed symbol inspection' >&2
        exit 1
    fi
    if assert_entitlements_are_release_safe \
        "$fixture_dir/missing" \
        "$entitlements_file" \
        "$codesign_error_file"
    then
        printf '%s\n' 'self-test failed: unreadable artifact passed entitlement inspection' >&2
        exit 1
    fi

    printf '%s\n' 'hot-reload release audit self-test passed'
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit 0
fi

run_audit "$@"