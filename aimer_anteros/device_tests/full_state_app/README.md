# Full-state target proof fixture

This is a standalone Aimer package for milestone-V target acceptance. It copies the proven stateful guest ABI pattern
into this package; it does not depend on the fixture crate. No target result is recorded by this README.

## Before a proof

From this directory, validate every intentionally compiling source snapshot before starting a watcher:

```sh
./scripts/variant.sh check
cargo test
```

`check` covers `initial`, `widget-body`, `schema-migration`, `callback-rebind`, `initial-build-trap`, and `recovery` by
checking the application library for `wasm32-unknown-unknown`. It deliberately excludes `compile-failure`, restores the
source snapshot that was active when it started, and never changes `Cargo.toml`. Run it only before a live proof because
validation necessarily replaces `src/guest.rs` several times.

Reset both watched contracts before launching:

```sh
./scripts/variant.sh apply initial
printf '%s\n' native-contract-v1 > native/contract.marker
```

## Target invocation

Run the selected target from this directory with the same CLI target selection used for a normal Aimer application:

```sh
aimer +nightly run -Z hot-reload
```

Select macOS, Windows, Linux, iOS Simulator, physical iOS, or Android through the CLI/device selection for that run.
The explicit guest metadata is in `aimer.toml`; the package exposes `FullStateGuest` and `HOT_RELOAD_LIMITS` from its
library target. The native shell is target-gated out of the generated WASM guest, while the CLI enables
`aimer/wasm-hot-reload` only for the native host build.

## Ordered acceptance actions

Do not restart the app between these actions. Each helper invocation atomically replaces exactly one watched contract:
guest variants replace only `src/guest.rs`, while `native-marker` replaces only `native/contract.marker`. The snapshots
remain under `variants/`, outside both watched roots.

| Step | Action | Required observation |
|---:|---|---|
| 1 | Launch the target, then press the proof surface at least three times. | `FULL STATE / INITIAL`; `counter: 3` (or the exact count pressed); generation 1 commits and the monochrome surface brightens with each press. |
| 2 | `./scripts/variant.sh apply widget-body` | One guest update; `FULL STATE / BODY CHANGED`; the same counter remains; button still adds 1. |
| 3 | `./scripts/variant.sh apply schema-migration` | One guest update; terminal names the required-state migration; `FULL STATE / SCHEMA V2 MIGRATED`; counter remains. |
| 4 | `./scripts/variant.sh apply callback-rebind`, then press the proof surface once. | `FULL STATE / CALLBACK REBOUND`; counter first remains, then rises by 10; callback ID remains `[0x22; 16]`. |
| 5 | `./scripts/variant.sh apply compile-failure` | Rust compilation fails with `intentional full-state fixture compile failure`; prior UI and `+10` behavior remain active. |
| 6 | `./scripts/variant.sh apply initial-build-trap` | Guest compiles, candidate build traps with `intentional full-state initial-build trap`; prior generation remains active. |
| 7 | `./scripts/variant.sh apply recovery` | One guest update commits; `FULL STATE / RECOVERED`; counter remains and button still adds 10. |
| 8 | Perform the required transport interruption/reconnect for the target. | Outstanding terminal status is recovered without app restart; recovered UI/state remain active. |
| 9 | `./scripts/variant.sh apply native-marker` | No guest build/push; CLI reports native restart required because native host source changed. |
| 10 | Stop the CLI. | Target route, session material, listener, and app are cleaned up as required by the proof procedure. |

The counter is required state. Schema v1 stores one counter byte; schema v2 stores that byte plus tag `2`, and the
candidate explicitly migrates v1 to v2. Callback variants retain `CALLBACK_ID` while changing the registered function’s
step from 1 to 10. The root shade independently exposes counter changes when target text rendering is unavailable:
counter zero is `0x50`, each increment adds `0x20`, and values saturate at `0xE0`. Compile and trap failures are recovery
checks, not successful commits.

## Helper contract

```sh
./scripts/variant.sh apply initial
./scripts/variant.sh apply widget-body
./scripts/variant.sh apply schema-migration
./scripts/variant.sh apply callback-rebind
./scripts/variant.sh apply compile-failure
./scripts/variant.sh apply initial-build-trap
./scripts/variant.sh apply recovery
./scripts/variant.sh apply native-marker
```

An omitted or unknown variant exits non-zero without changing a watched file. Applying a snapshot that is already
active may still emit a filesystem replacement notification; follow the ordered scenario rather than reapplying a step.