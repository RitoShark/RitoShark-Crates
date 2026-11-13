# Testing and CI coverage

## What CI runs

| Lane | Environment | Checks |
|---|---|---|
| Core (`ci.yml`) | Windows, Rust pinned in `rust-toolchain.toml` (currently 1.96.0) | Locked workspace build and default-feature tests, excluding `rs_python` |
| Optional features (core job) | Same | `rs_wad/parallel` and `rs_rman/verify` tests |
| Facade (core job) | Same | Compile `ritoshark` with no default features, then with only `bin,wad` |
| Lints (core job) | Same | All-target/all-feature Clippy excluding Python; workspace rustfmt |
| Python (`python-bindings.yml`) | Windows, Python 3.11 | Build and install a wheel, then run pytest |

Python checks are deliberately separate from the core Rust job because building
`rs_python` requires a Python environment. Changes to the root Cargo manifests,
lockfile, and toolchain also trigger the Python pull-request workflow.

This matrix does not establish Linux/macOS runtime support, compatibility with every
Python version, every feature combination, or a tested minimum Rust version.
`Cargo.toml` declares Rust 1.85, but CI currently verifies only the pinned 1.96.0
toolchain. Do not treat the declared minimum as a validated dependency/MSRV promise.

## Reproduce the checks

```powershell
cargo build --workspace --exclude rs_python --locked
cargo test --workspace --exclude rs_python --locked
cargo test -p rs_wad -p rs_rman --all-features --locked
cargo check -p ritoshark --no-default-features --locked
cargo check -p ritoshark --no-default-features --features bin,wad --locked
cargo clippy --workspace --exclude rs_python --all-targets --all-features --locked -- -D warnings
cargo fmt --all --check
```

For Python, follow [the binding instructions](../crates/rs_python/README.md) or
the steps in its workflow. A passing core job is not evidence that Python tests ran.

## Real files and skipped coverage

Real game assets are not committed. Many integration tests return successfully when
their local fixture paths are absent. Therefore a green hosted job establishes the
synthetic checks and any available fixtures, **not** the complete real-file corpus.
Some tests are explicitly ignored; notably the BIN PBE hash-width probe is not part
of an ordinary test run. Review the test output and fixture paths before claiming
coverage for a specific version.

For corpus validation, provision the files referenced by each crate's
`tests/real_files.rs` and related tests, then run the suite with `-- --nocapture` and
check skip messages. There is currently no required corpus runner or fixture manifest
that fails the build when those assets are missing. Add that lane when a maintained,
legally available fixture set and runner are ready. Test coverage here describes
which checks run; no line/branch coverage percentage is currently measured.

## Regression priorities

Save tests verify that serializer errors and failed replacement preserve existing
data and clean up temporary files. WAD tests cover TOC-only serialization, payload
bounds, changing builder input, exact decompressed lengths, and subchunk coverage.
Keep adding small synthetic reproductions alongside fixes; reserve real fixtures
for compatibility questions that synthetic data cannot answer.
