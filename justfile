# Dioxus CLI (pinned to the cargo-installed binary so Deno's `dx` on PATH can't shadow it)
dx := join(home_directory(), ".cargo/bin/dx")

# Default: list available recipes
default:
    @just --list

# Run an example with hot-patching (dx serve)
live example="into_jfk":
    {{dx}} serve --example {{example}} --hotpatch


# Bounce an example to a WAV offline (no audio device)
render example="progressive" bars="16":
    cargo run --release --example {{example}} -- --render {{bars}} {{example}}.wav
