# Default: list available recipes
default:
    @just --list

# Run an example with hot-patching (dx serve)
live example="into_jfk":
    dx serve --example {{example}} --hotpatch

