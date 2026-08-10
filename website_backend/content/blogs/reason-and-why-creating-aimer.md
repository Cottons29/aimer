# Why I Am Creating Aimer

People keep asking me the same question: why write a whole GUI framework when the world already has plenty of them? Why
not using Leptos, Why not using Flutter? It is a fair question. Writing a framework is slow, unglamorous work, and most
of it happens far below the part anyone actually sees. So let me answer it properly: not with a feature list, but with
the reasons that made me start.

### What I Tried First

I did not start from zero out of enthusiasm. I went shopping first.

- **Flutter**: the widget model is genuinely good, and honestly it is what Aimer's tree is inspired by. What I did not
  want was the runtime, the second language, and the FFI seam described above.
- **egui / iced**: Rust-native, which was the right instinct, but the architectures did not match how I think about UI.
  I wanted a persistent, composable tree I could reason about, not a redraw loop or an Elm-shaped message pipeline for
  every interaction.
- **Tauri and Electron**: whether shipping a browser engine or wrapping the OS's native webview, it's still a lot of
  machinery to bring in before writing the first line of app code. And I'd still be writing UI in a different language
  than my logic.
- **Qt / GTK bindings**: mature, battle-tested toolkits, and I respect them. But the code stops feeling like Rust. You
  get build setup, FFI, lifetimes bolted onto an object model that predates the borrow checker by decades.

Each of them solves a real problem well. None of them solved *my* problem, which was: one language, one codebase, one
model, on every platform, with performance I control.

### One Model, Both Sides

The dream is small and stubborn: **I want the frontend and the backend to share the same model.**

Say I have a `User`. On the server it is a Rust struct: typed, validated, exhaustively matched. Then I move to the UI,
and if the UI is Dart, I write that struct again as a class. Same fields, same shapes, a second source of truth that
starts drifting the moment someone adds a column. Field renamed on the server? The Dart class compiles happily and lies
to me at runtime.

If both sides are Rust, that whole category of work disappears:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: String,
}
```

One definition. The API returns it, the widget tree renders it, and the compiler is the thing keeping them honest. No
mirrored DTOs, no codegen step in the middle, no "the field is nullable on the backend but not in the UI" bug. Strict
types all the way through, end to end.

### Rust Next To A Garbage Collector Hurts

Before Aimer I did the pragmatic thing everyone recommends: Flutter for the UI, Rust for the core logic, FFI in between.
On paper it is the best of both worlds. In practice I spent a lot of evenings debugging the seam instead of building the
app.

The two languages disagree about who owns memory, and the disagreement is silent. Rust hands out a handle expecting it
to stay alive; the GC on the other side decides that object is unreachable and reclaims it. Now Rust is looking for
something that no longer exists. Nothing in either language is wrong on its own: the boundary is wrong. And every crash
lives exactly there, in the place with the worst debugging story of the whole stack.

> Two ownership models, one process. Sooner or later one of them wins an argument you did not know was happening.

At some point I stopped treating that as a problem to fix and started treating it as a design answer: remove the
boundary. If the UI is Rust too, there is nothing to marshal, nothing to keep alive across a bridge, nothing to guess
about.



### Performance Is Not A Later Problem

Aimer is a GUI framework, so performance is not a feature: it is the whole substrate. A frame budget is roughly sixteen
milliseconds, and everything the framework does is spending someone else's money inside that budget. That belief is the
one I refuse to trade away, even when it makes my life harder.

It is why **Cupid**, the renderer, batches draw calls instead of issuing them one by one, and why rounded corners,
borders and clipping happen on the GPU rather than in a software rasterizer. It is why pointer capture and text
processing got dedicated optimization passes instead of a "good enough for now" implementation.

Single codebase across desktop, mobile and web is the second non-negotiable. If a design only works on one platform, it
is not a design: it is a workaround waiting to be paid for.

### A Framework Without Tooling Is Just A Crate

This is the part I underestimated at the start, and the part I am most stubborn about now.

> Without tooling, a framework is just a crate. You scaffold the FFI by hand, you wire the bundling by hand, and then
> you do it again for macOS, Android, iOS, Windows, Linux, and whatever ships with a screen next.

`cargo run` still works: an Aimer app is an ordinary binary crate, and on your own desktop that is the whole story. It
stops being the whole story the moment the target is not your desktop. A phone build is not a compile, it is a pipeline:
compile for the right triple, drop the artifact into a platform scaffold, package it, sign it, push it, launch it, then
attach to the log stream. Every platform spells those steps differently, and none of them is interesting work.

So `aimer run` owns the pipeline end to end: **build, assemble, launch.** One command, same command, whichever platform
you pointed it at. `aimer build` stops after the artifact, `aimer assemble` produces the distributable bundle, and
`aimer create` writes a project that already has every platform scaffold in place, because "set up Gradle yourself" is
not an onboarding step, it is a reason to close the tab.

Then there is `aimer doctor`, which exists because of a specific kind of wasted evening. A missing NDK, a target you
never added with `rustup`: those failures used to surface deep inside somebody else's build system, as an error message
about a path you have never heard of. `doctor` asks the questions up front and tells you what is missing in one screen,
in plain words, before you spend twenty minutes learning to read Gradle output.

The small things matter too. Shell completions are generated from the running binary rather than baked into a static
snapshot, so a subcommand I add today shows up in your shell after a rebuild, with no completions file to regenerate and
no chance of the script drifting from reality.

Flutter's tooling is the reference point here, and I say that gladly: it is simple, it does what it says, and it stays
out of the way. The one place I deliberately went the other direction is output. I would rather watch the build log
scroll than watch a spinner tell me nothing. When something breaks on a platform you barely control, the log is the only
thing you have.

And the honest part: this is the least glamorous code in the whole project. There is no elegant abstraction hiding in
platform scaffolding. It is templates, build configuration, and a long tail of builds that fail for reasons specific to
one SDK version, fixed one at a time. I keep doing it because the alternative is asking every user to solve the same
problems privately, forever.

### Why Builders, Not Macros

Aimer used to be macro-based. It looked beautiful and Flutter-ish:

```rust
Container!(
    child: Text!(
        "Hello World!",
        text_align: TextAlign::MidCenter,
    )
);
```

And it was, in daily use, miserable. An error anywhere inside a macro highlights the whole macro; you get a wall of red
and then read prose to find out which field you got wrong. Refactoring is manual labour: rename one field and you are
grepping through call sites doing search-and-replace by hand, because the tooling cannot see through the expansion.
Completion is gone. Go-to-definition is gone. Everything an IDE gives you for free on ordinary Rust code, macros quietly
take back.

So Aimer moved to the builder pattern:

```rust
Container::new()
  .child(
    Text::new("Hello World!"
      .text_align(TextAlign::MidCenter)
      .text_style(
        TextStyle::new()
          .color(Color::BLACK)
    )
  ),
)
```

Slightly more punctuation, dramatically better life. Every method is a real method: hover it, jump to it, rename it and
let the compiler find every caller. Errors point at the argument that is actually wrong. And it turns out chained
builders read as declaratively as any macro DSL once you have written a screen or two: the structure of the call mirrors
the structure of the UI. Marking the builder methods `#[inline]` means the ergonomics cost nothing at runtime.

> Declarative is a property of how the code reads, not of whether it is written inside a macro.

### The Hardest Part Was Not Technical

If you asked me to name the hardest stretch, it was not a bug. It was the period where I had no direction: no clear
sense of what Aimer was supposed to be, so every decision felt arbitrary.

My instinct then was to be lazy in the good sense: pull in a crate, let someone else's abstraction make the decision for
me, move on. Sometimes that works. In the rendering path it kept not working. Text was the clearest case: shaping and
rasterizing glyphs sits directly on the frame budget, and I needed control over caching, batching and how the atlas
behaves under pressure: control an external abstraction was never going to hand me. I hit that wall more than once
before accepting the obvious conclusion and building the text pipeline myself.

That trade is real and worth stating plainly: doing it yourself is harder, slower, and entirely your fault when it
breaks. It also means that when something is slow, I can actually go fix it, instead of filing an issue and waiting. For
a framework whose entire premise is performance, that is not optional.

### Who This Is For, And Where It Is Going

Aimer is for developers building GUI applications in Rust who do not want to give up either performance or a single
codebase to get there.

Looking back over the last year, it has come further than I expected: the widget system, the layout engine, Cupid's
rendering path, text, animation, routing, the CLI. It is both things at once, and I have stopped pretending otherwise:
it is how I learn this domain deeply, and it is a serious product I intend to keep shipping.

If any of this sounds like a problem you also have, try it. The source and the `jaime` examples are
on [GitHub](https://github.com/Cottons29/aimer): build something small, break it, and tell me what broke. Honest bug
reports from people using it for real work are worth more to me than stars.
