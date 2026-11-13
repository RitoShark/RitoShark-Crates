# Testing and CI coverage

## What CI runs

| Lane | Environment | Checks |
|---|---|---|
| Core (`ci.yml`) | Windows, Rust pinned in `rust-toolchain.toml` (currently 1.96.0) | Locked workspace build and default-feature tests, excluding `rs_python` |
| Minimum Rust (`ci.yml`, `msrv` job) | Windows x64 MSVC, declared `rust-version` (1.88.0) | Compile all core test targets/features, run library unit tests, check minimal facades and ABI3 Python-binding compilation |
| Optional features (core job) | Same | `rs_wad/parallel` and `rs_rman/verify` tests |
| Facade (core job) | Same | Compile `ritoshark` with no default features, then with only `bin,wad` |
| Lints (core job) | Same | All-target/all-feature Clippy excluding Python; workspace rustfmt |
| Corpus runner checks (core job) | Windows, PowerShell | Synthetic checks for preflight failures, cargo failures, skipped coverage, and empty/incomplete runs; no private corpus |
| Python (`python-bindings.yml`) | Windows, Python 3.11 | Build and install a wheel, then run pytest |

Python checks are deliberately separate from the core Rust job because building
`rs_python` requires a Python environment. Changes to the root Cargo manifests,
lockfile, and toolchain also trigger the Python pull-request workflow.

This matrix does not establish Linux/macOS runtime support, compatibility with every
Python version, or every feature combination.

## Minimum Rust version

The workspace minimum is **Rust 1.88.0**, validated with the checked-in lockfile on
`x86_64-pc-windows-msvc`. All crates inherit this common baseline. The regular
development, lint, and runtime-test toolchain remains pinned to 1.96.0.

The previous 1.85 declaration was inconsistent with both source and dependencies:
the animation reader uses [let chains, stabilized in Rust 1.88](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/),
and locked `image` 0.25.10 declares Rust 1.88.0. The locked Vorbis stack declares
1.87, and ICU dependencies declare 1.86. No dependency downgrade or lockfile change
was needed to establish the 1.88 baseline.

The `msrv` job reads `rust-version` directly from `Cargo.toml`, installs that full
version, and uses it explicitly instead of the development override. It compiles
all non-Python workspace test targets with all features, runs library unit tests,
and checks the facade with no features and with `bin,wad`. Integration tests are
compiled at the minimum; their runtime coverage remains in the normal pinned-toolchain
job and the local corpus runner.

Python bindings are checked with `PYO3_NO_PYTHON=1` and their existing ABI3 settings.
That validates Rust compilation without an interpreter; it does not link/install
a wheel or run Python tests at the minimum. The separate Python workflow retains
its existing runtime checks on the development toolchain.

Dependency updates must pass the minimum-version job or deliberately update the
declared baseline. This verifies the locked Windows graph, not every dependency
resolution a downstream consumer might select or other operating systems.

```powershell
rustup toolchain install 1.88.0 --profile minimal
cargo +1.88.0 test --workspace --exclude rs_python --all-targets --all-features --no-run --locked --target-dir target/msrv
cargo +1.88.0 test --workspace --exclude rs_python --all-features --lib --locked --target-dir target/msrv
cargo +1.88.0 check -p ritoshark --no-default-features --locked --target-dir target/msrv
cargo +1.88.0 check -p ritoshark --no-default-features --features bin,wad --locked --target-dir target/msrv
$previousNoPython = $env:PYO3_NO_PYTHON
try {
    $env:PYO3_NO_PYTHON = '1'
    cargo +1.88.0 check -p rs_python --locked --target-dir target/msrv
} finally {
    $env:PYO3_NO_PYTHON = $previousNoPython
}
```

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

For required corpus validation, use the runner below. Test coverage here describes
which checks run; no line/branch coverage percentage is currently measured.

## Required corpus mode

From the workspace root on Windows:

```powershell
./scripts/test-corpus.ps1
./scripts/test-corpus.ps1 -Crate rs_tex,rs_mapgeo
./scripts/test-corpus.ps1 -CheckOnly
```

If local script execution is disabled, a process-only invocation is available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-corpus.ps1
```

[The manifest](fixtures.json) pins 59 reference files by name, byte length, and
SHA-256, and lists 20 integration-test targets across 11 format crates. Provision
your own permitted copies under the workspace's ignored `Sample-Files/` directory,
using the exact names in the manifest. Current live files may differ from these
reference snapshots; renaming a newer file does not make it an equivalent fixture.
The runner reads reference files in place without downloading or publishing assets.
Existing tests may write derived outputs to temporary directories.

Preflight verifies every selected crate's files before running any tests. Missing,
unreadable, wrong-sized, or changed files fail with their paths. Unknown crate names
and missing test targets also fail. `-CheckOnly` stops after preflight and explicitly
does **not** claim compatibility-test success.

The runner invokes each crate's listed targets with `--all-features --locked`,
`--nocapture`, and one test thread. Cargo failures, reported skips, and absent or
zero-test summaries fail the run. This reuses the existing binary/text round-trip,
decode, edit, pose, and container tests; ordinary `cargo test` still permits absent
private fixtures. Captured skip diagnostics are a secondary check, not a general
proof that every branch of a test ran.

| Crate | Required test targets |
|---|---|
| `rs_anim` | `real_files`, `pose` |
| `rs_audio` | `real_files`, `wem`, `hirc`, `encode_vorbis`, `sweep` |
| `rs_bin` | `real_files` |
| `rs_luabin` | `roundtrip` |
| `rs_mapgeo` | `real_files`, `edit` |
| `rs_mesh` | `real_files` |
| `rs_rman` | `real_files`, `parameters`, `file_size_width` |
| `rs_rst` | `real_files` |
| `rs_tex` | `real_files` |
| `rs_troybin` | `roundtrip`, `inibin` |
| `rs_wad` | `real_files` |

This is a named baseline, not every private test in the repository. It excludes
the ignored BIN PBE probe, BIN blend-key/private absolute-path fixtures, the local
malformed-TEX project collection, CLI/Python fixtures, and opt-in network tests.
The audio sweep additionally examines other `.bnk`/`.wpk` files in `Sample-Files/`;
only the baseline files are pinned by the manifest. WAD tests inspect selected
payloads, not every file in the live installation.

When adding a corpus-backed test, add its target and all required files to the
manifest in the same change. When replacing a snapshot, review the expected
behavior before updating its size/hash, then run the affected crate's corpus tests.
Do not update fingerprints merely to silence a failure. File fingerprints establish
identity, not format correctness; the integration tests establish the tested behavior.

Hosted CI tests the runner using temporary synthetic files and a stub cargo command:

```powershell
./scripts/test-corpus-runner.ps1
```

The actual private-corpus run is currently local. A private runner with provisioned
assets can invoke the same command, but no such hosted/self-hosted job is configured
or claimed here. Neither the private corpus nor its captured output is uploaded.

## Regression priorities

Save tests verify that serializer errors and failed replacement preserve existing
data and clean up temporary files. WAD tests cover TOC-only serialization, payload
bounds, changing builder input, exact decompressed lengths, and subchunk coverage.
Lookup/extraction tests cover unsorted tables, duplicate hashes, repeated requests,
and TOC-only indexed reads, in both sequential and parallel runs. MAPGEO synthetic
tests cover half-float attributes, missing buffer references, and failed multi-stream
edits preserving the complete map, including shared buffers.
World-space MAPGEO tests cover rotation/translation, non-uniform scale, shear,
reflected winding, split half-float streams, and invalid export inputs. A fixture
test exports the first model of each available map sample without changing its source.
MAPGEO edit regressions also reject non-finite attributes and incomplete triangles
before either add or replace changes a map. Half-float overflow tests cover all three
packed layouts and failure in a later stream without partial mutation. Positive
checks retain tiled UVs, zero normals, overlapping/empty draw ranges, and gaps;
existing codec tests retain explicit half NaNs/infinities and subnormals.
Texture-header tests verify bounded reads, all truncated header prefixes, DDS flags
and structure sizes, unknown format handling, and raw TEX metadata retention,
including the observed format-23 volume header. CLI tests inspect TEX/DDS headers
without pixel payloads and verify that an unknown TEX mip count is JSON `null`.
TEX mip-layout regressions cover valid rectangular chains, ambiguous short payloads,
surplus bytes, allocation preflight, and explicit largest-mip extraction. The
`audit_tex_mips` example supports read-only checks against supplied WADs; see the
[audit results and coverage limits](../crates/rs_tex/docs/mip-layout-audit.md).
Keep adding small synthetic reproductions alongside fixes; reserve real fixtures
for compatibility questions that synthetic data cannot answer.

Texture decode-boundary tests cover zero/extreme dimensions, short encoded payloads,
invalid DDS mip/array counts, exact rectangular mip offsets, DX10 cube arrays, and
single-surface DDS-to-TEX adoption. A deterministic sweep exercises every truncated
prefix of a synthetic DDS plus 592 single-byte header mutations through the public
decoders. These tests run in ordinary CI without assets or a fuzzing toolchain; they
are bounded regression checks, not a claim of coverage-guided fuzzing or exhaustive
malformed-input safety.

Equivalent deterministic sweeps cover PROP/PTCH and the WAD header/table. WAD
readers check table byte availability before reserving the declared chunk count;
the same path serves full and TOC-only mounts. Count regressions cover v2, v3.3,
and v3.4. These small mutation sweeps strengthen normal CI without introducing a
separate fuzzing runtime. Dedicated coverage-guided campaigns remain follow-up work.

## WAD performance baseline

Run the asset-free benchmark in an optimized build:

```powershell
cargo bench -p rs_wad --bench selection --locked
cargo bench -p rs_wad --bench selection --features parallel --locked
cargo test -p rs_wad --bench selection --all-features --locked -- --test
```

The stable-Rust harness uses synthetic archives of 1,000, 10,000, and 50,000 stored
128-byte chunks. It reports CSV timings for 256 evenly distributed linear lookups,
the same lookups against a prebuilt borrowed index, index construction, selected
extraction, and full extraction. Archive construction and correctness assertions
are outside the timed region. Index construction has its own measurement; lookup
timings do not include it. Extraction includes allocating and dropping its results.

There is one warm-up and at least 250 ms of sampling per case. The last command runs
a short smoke check, also included in CI. Compare matching toolchains, feature flags, machines, and workloads;
the harness deliberately has no CI timing threshold. Stored chunks isolate lookup,
selection, and copying costs: these figures do not predict zstd, disk I/O, live WAD
loading, or large-payload performance. No benchmark dependency or game data is needed.

All fixture references use the workspace directory spelling `Sample-Files` so that
the same paths also resolve on case-sensitive filesystems. This is path consistency,
not a claim that Linux/macOS build or corpus coverage has been established.

## Batch audio replacement

`rs_audio/tests/batch_edit.rs` checks transactional failure, repeated request IDs,
duplicate container IDs, untouched media/metadata, empty payloads, exact no-op padding
preservation, and malformed BNK media tables. The existing required `real_files` target
also swaps media in three pinned BNK fixtures (v134/v145) and one WPK, validating
serialization, unrelated data, names, and slots. It needs no new fixture files.

```powershell
cargo test -p rs_audio --lib --test batch_edit --test real_files --locked
cargo bench -p rs_audio --bench batch_replace --locked
cargo test -p rs_audio --bench batch_replace --locked -- --test
```

The synthetic benchmark replaces all payloads in 32/128/512-entry banks with 4 KiB
stored byte buffers. It compares repeated `replace_wem` calls against one
`replace_wems` call and verifies identical results. Setup/cloning and result disposal
are outside the timer. The output averages ten operations per method; CI runs one
smoke iteration without a timing threshold. This measures container edits only,
not audio encoding, disk saves, WPK behavior, or general real-bank performance.
