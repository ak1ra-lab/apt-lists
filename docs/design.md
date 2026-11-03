# Design and development

## Design notes

* One `libapt-pkg` cache is opened per invocation (via `oma-apt`), packages
  are iterated once, and repository filtering happens in memory. No
  `apt-cache` subprocesses are spawned, and `/var/lib/apt/lists` is never
  parsed by hand.
* The `Package → installed Version → VersionFile → PackageFile` relation is
  taken directly from libapt-pkg, which is what makes exact
  name+version+architecture matching possible without re-implementing any
  index logic.
* The dpkg status database also appears as a `PackageFile` (index type
  `Debian dpkg status file`, never downloadable); it is excluded from
  repository provenance but is what makes locally installed packages visible.
* The tool is strictly read-only: `Cache::update()` is never called, nothing
  is marked for install/remove, no locks are taken.
* Human tables always use the same columns: a `--repo` filter narrows the
  rows, never the columns, and `--repos` prints one row per repository and
  suite so the table stays narrow. Cells are single whitespace-free tokens
  (multi-values joined by `,`), so `--no-headers` output pipes straight into
  `sort -k<n>`/`awk`.
* Output is written through explicit, error-checked writes: when the reader
  of a pipe goes away (`apt-lists -i | head`), the tool exits quietly with
  status 141 (128 + SIGPIPE) instead of panicking.

## Code style and safety

The repository enforces a strict style/safety gate locally and in CI
(`just check`):

* `cargo fmt --check` — no formatting differences allowed.
* `cargo clippy --all-targets -- -D warnings` — the lint configuration lives
  in the `[lints]` tables of `Cargo.toml`: `clippy::all`, `clippy::pedantic`
  and `clippy::cargo` are enabled, plus `missing_docs`.
* `unsafe_code` is denied: this crate contains no `unsafe` blocks. All
  libapt interaction goes through the safe `oma-apt` wrappers. The single
  sanctioned exception is one `unsafe { env::set_var }` call in the test
  fixture (setting `APT_CONFIG` before libapt initializes; commented in
  place).
* CI (`.github/workflows/ci.yaml`, GitHub Actions) runs fmt, clippy, the test
  suite and `cargo package` on every push and pull request. Dependabot keeps
  Cargo and Actions dependencies current.

## Testing

The test suite does not touch the host system's APT state. Each test builds
an isolated APT root (own `sources.list`, package lists, dpkg status, auto
install marks and cache directories) and points `libapt-pkg` at it via the
documented `APT_CONFIG` environment variable. Covered scenarios include:
same suite on two official repositories (archive + mirror) staying distinct,
exact version matching (positive and negative), multiple providers of one
exact version, architectures (`amd64`/`i386`/`all`), candidate ≠ installed,
manual vs auto installed, status-only packages, config-files state, unknown
and ambiguous repository selectors, shell completion generation, uniform
table columns across modes, the compact `--repos` table, the JSON output
shapes, and broken-pipe handling (`apt-lists -i | head` exits quietly with
status 141) — both at the library level and end-to-end through the CLI
binary.

```console
just test    # or: cargo test
```

## Releasing

Releases are tag-driven (`vX.Y.Z`); see [releasing.md](releasing.md). The
`release` workflow verifies the tag against `Cargo.toml`, reuses the `ci`
workflow, and publishes the source package to crates.io via Trusted
Publishing. No binary artifacts or GitHub Release are produced.
