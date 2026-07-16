# AimerMarkdown — Development Guide

Instructions for AI coding assistants and human contributors working on the Aimer codebase.

**Never give up on the right solution.**

---

## Project Overview

AimerMarkdown is a Rust library that provides a set of tools for render the Markdown file in an Aimer framework.

## Golden Rules

- **Use CodeGraph to understand code.** It is fast and always safe for reading/navigating the
  codebase. Prefer it before opening files blindly.
- **Use the IDE (IDEA/CLion) integration to edit code** when connected — it is the safest, fastest
  path for refactors and renames.
- **Never write "Lazy Senior Dev" code.** Do not merely patch the symptom with spaghetti that other
  developers will curse. Solve the actual problem cleanly.
- **Follow Test Driven Development.** Write the failing test first, then the code that makes it pass.

# Implementation Steps

- [ ] Add a public RichText/span widget with mixed-style wrapping and link hit regions.
- [ ] Parse Markdown into an internal document tree.
- [ ] Map block nodes to Aimer layout widgets and inline nodes to rich-text spans.
- [ ] Add a configurable theme, link callback, image resolver, and selectable/copyable code blocks.
