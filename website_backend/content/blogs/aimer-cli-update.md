# New Updated Aimer CLI

Aimer CLI is a tool provided by the `Aimer` framework for scaffold build assemble and run the application. After a
period of time using it, now we have updates for the commands you don't type every day, the ones that matter when you
stop running the app and start shipping it, or when the project you scaffolded months ago has fallen behind the
templates.

`create`, `run` and `doctor` are covered in *Better DX with Aimer and News*. This post is about the other four.

## `build`, compile, don't launch

```bash
aimer build                      # uses [build].default_target from aimer.toml
aimer build --target android
aimer build --release
```

`build` resolves its target in one order and says so when it can't: the `--target` flag first, then
`[build].default_target` in `aimer.toml`, and an error naming the valid targets if neither is there. It then runs the
right compiler invocation for that platform with inherited stdio, so what you see is cargo's own output rather than
something re-formatted, which is what a CI log wants.

What it does *not* do is package anything. `build` compiles the Rust side and stops. That separation exists because
compiling is the part you repeat and packaging is the part you don't.

## `assemble`, the distributable bundle

```bash
aimer assemble macos --release
aimer assemble android
aimer assemble web
```

`assemble` is where the platform bundle actually comes out: a `.app` on macOS and iOS, an `.apk` on Android, the static
`dist/` tree for the web. It is non-interactive by design, a platform argument, an optional `--release`, no picker, no
TUI, each step running synchronously with inherited stdio.

The important part is what it shares. The packaging steps are the *same* code `aimer run` drives; the only difference is
the reporter they are handed, the console's, or a plain stdio one. A bundle assembled in CI is therefore built by the
same path as the bundle you launched on your desk five minutes earlier, which is the only way "works on my machine" ever
stops being a sentence people say.

## `migrate`, catch a project up to the current templates

```bash
aimer migrate ios
aimer migrate all
```

The platform scaffolds under `builds/` are generated, not hand-written, and they move with the CLI: a new entitlement, a
changed Gradle plugin, a fixed linker flag. A project created three versions ago keeps whatever it was born with.

`migrate` regenerates the scaffold for one target, `macos`, `windows`, `linux`, `android`, `ios`, `web`, or `all`,
using the templates bundled in the CLI you have installed right now. It reads `name` and `group` back out of
`aimer.toml`, so the regenerated bundle identifier and application id are the same ones your project already ships
under, and it refuses to run outside an Aimer project root rather than scattering folders wherever you happened to be.

An unknown target is rejected with the list of the valid ones instead of a stack trace.

## `completions`, a script you install once

```bash
aimer completions zsh              # print it
aimer completions zsh --install    # write it where the shell looks
```

The generated script is **dynamic**. It is not a snapshot of the command tree at the moment you ran the command: it
registers the binary itself as the completer, so every time the shell asks for a completion it asks the *running*
`aimer` on your `PATH`. Add a subcommand, rebuild, and it completes, no regeneration, no stale suggestions for a flag
that was renamed.

`--install` writes the script into the shell's conventional per-user completion directory and prints the activation
hint. Without it the script goes to stdout so you can `source` it or put it wherever your dotfiles keep such things.

## `clean`, remove the artifacts, keep the project

```bash
aimer clean
```

Two kinds of output pile up in an Aimer project: cargo's `target/`, and everything the platform builds leave inside
`builds/`, the macOS and iOS `build/` and `Libraries/` directories, `builds/web/pkg` and its `node_modules`, the
Android `app/build` and the `jniLibs` the native libraries are staged into.

`clean` removes exactly those, naming each one as it goes (and saying so when there is nothing to remove), then hands
`target/` to `cargo clean` rather than deleting the directory itself, cargo knows where the target directory really is
in a workspace, and this tool does not need to guess. Your generated Xcode project, your Gradle files and your
`aimer.toml` are untouched: `clean` deletes build output, never scaffolding. Scaffolding is `migrate`'s job.

> The CLI lives in the `aimer_cli` crate on [GitHub](https://github.com/Cottons29/aimer), and `aimer --help` is
> generated from the same command tree the completions are.
