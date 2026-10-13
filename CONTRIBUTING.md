# Contributing to Inkframe

Thanks for helping! Bug reports, features, docs and test files are all
welcome.

## Setup

1. Install Rust with [rustup](https://rustup.rs).
2. Install ffmpeg if you'll touch video export (`brew install ffmpeg` on
   macOS, `choco install ffmpeg` or `winget install ffmpeg` on Windows).
3. `cargo run` builds and launches the editor.

Read [ARCHITECTURE.md](ARCHITECTURE.md) before a larger change. It explains
which crate a change belongs in.

## Before opening a pull request

CI runs these three commands, so run them locally first:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Code style

- **Explain the why, not the what.** Comment the reasoning a reader couldn't
  get from the code itself, such as a math derivation, an ffmpeg quirk, or why
  something happens in a particular order. Skip comments that restate the code.
- **Keep `inkframe-core` free of UI and rendering.** If you need egui or
  tiny-skia types there, the code probably belongs in another crate.
- **Test behaviour.** A test named like `hold_jumps_on_the_next_keyframe`
  documents the rule it checks.
- **Prefer small, focused pull requests.** Several small PRs are easier to
  review than one large one.

## UI changes

Include a screenshot in UI pull requests. You can capture one without
touching the screen:

```sh
INKFRAME_SCREENSHOT=shot.png INKFRAME_SCREENSHOT_FRAME=10 \
INKFRAME_SCREENSHOT_SELECT=Ball INKFRAME_SCREENSHOT_ONION=1 INKFRAME_SCREENSHOT_TOOL=Selection \
  cargo run -- examples/bouncing-ball.inkf
```

## Licensing

By contributing, you agree that your contributions are dual-licensed under
the MIT and Apache-2.0 licenses, the same as the rest of the project.
