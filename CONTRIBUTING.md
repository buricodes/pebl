# Contributing

Thanks for helping with pebl.

1. Fork the repo and create a branch from `main`.
2. Make your change. Keep to the rules in `CLAUDE.md`: no code comments, colours only from `ui/theme.rs`, and add a snapshot test in `src/ui/screen.rs` for any new screen.
3. Check it locally:

   ```sh
   cargo fmt
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

4. Open a pull request against `main`. CI runs the same checks, and a maintainer reviews it.

`main` is protected: changes only land through reviewed pull requests, and only maintainers publish releases.
