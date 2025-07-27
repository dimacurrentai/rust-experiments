# High-level

Never remove existing tests, or the logic from them -- your job is to make existing tests build and pass if they fail to.
Write no comments, keep the code self-descriptive.
Respect Rust formatting rules in `../../rustfmt.toml`. No need to bring this `rustfmt.toml` into this project's directory from `../../`.
Keep Rust edition at 2024 in `Cargo.toml`.

# When Contributing

Only make small, self-contained changes.
Make sure they are readable and understood in isolation -- with no comments, from the code alone!
Do not add doctests for the sake of adding doctests, although if a standalone function calls for a doctest -- do add it!
Feel free to use `cargo doc` to examine the docs, but do not add `--open` to `cargo doc`!
If the project is using `ntest`, add `#[ntest::timeout(5000)]` to new tests. Do not modify or add/remove timeouts for existing tests.

# Before Committing

Every source file including `Cargo.toml` should end with a newline.
Run `cargo fmt` to ensure the style is consistent.
Run `cargo test` to ensure all tests pass.
Run `cargo check` to verify compilation.
Remove unused dependencies.
Ensure no build warnings.
