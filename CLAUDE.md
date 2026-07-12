# Guidelines

- **Think like experts**: Consider who the best team of specialist experts to solve the problem would be, and reason like them.
- **Favor coherency**: Remove indirection and unnecessary abstraction. Direct, obvious code wins.
- **Think holistically**: Before implementing, consider both high-level architecture and low-level details—how does this cleanly integrate with existing code and APIs?
- **Implement the right way**: Approach changes as if building from scratch. Avoid bandaids, patches, or working around existing architecture.
- **Optimize for correctness over comfort**: Prefer the clean architectural move over incremental patches—even if it means a large or complex refactor. Make the change thoughtfully and finish it properly.
- **Do not create simplified versions**: Push through and do the intended way—even when the path is difficult or complex.
- **Verify your work**: Run `cargo test` in the relevant module after implementing.
- **Ignore clippy warnings**: Don't fix or address `cargo clippy` lints unless explicitly requested.
- **Use modern Rust module style**: Prefer `foo.rs` + `foo/` siblings over `foo/mod.rs`.
- **Avoid re-exports**: Import from the defining module directly rather than re-exporting from `lib.rs`.