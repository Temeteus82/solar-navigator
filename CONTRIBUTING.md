# Contributing

Thanks for contributing to Solar Navigator.

## Development Setup

1. Install stable Rust (`rustup`).
2. Clone the repository.
3. Run checks from the project root:

```bash
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

For portable mode:

```bash
cargo check --all-targets --no-default-features
cargo clippy --all-targets --no-default-features -- -D warnings
cargo test --all-targets --no-default-features
```

## Pull Request Guidelines

1. Keep PRs focused and small when possible.
2. Include tests for behavior changes when practical.
3. Update docs when flags, scripts, build steps, or controls change.
4. If you add dependencies, ensure they have compatible licenses.

## Assets and Third-Party Content

If you add external assets/code/data:

1. Document source and license in `THIRD_PARTY_NOTICES.md`.
2. Update `ASSET_ATTRIBUTION.md` when asset attribution is required.
3. Keep original attribution metadata where applicable.

## License of Contributions

By submitting a contribution, you agree that your contribution is licensed
under this repository's MIT License.
