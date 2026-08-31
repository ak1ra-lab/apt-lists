# AGENTS.md

Read-only, repository-aware companion to `apt list`: a single Rust crate
(`apt-lists`) that answers "which installed package versions are currently
provided by a specific APT repository?" via `oma-apt` (libapt-pkg bindings).
See README.md for provenance semantics and examples.

## Commands

- `just check` - full CI gate: fmt-check + clippy + tests. Run before finishing.
- `cargo test --test fixture <name>` - run one integration test; tests live in
  tests/fixture.rs (unit tests: `cargo test --lib`).
- `just lint`, `just fmt`, `just build`, `just completion <shell>`.
- Build requires `libapt-pkg-dev` (oma-apt links libapt-pkg) and Rust 1.85+.

## Lints and style gates

- Lint config lives in `Cargo.toml [lints]`: `clippy::all`, `clippy::pedantic`,
  `cargo`, plus `missing_docs`. CI runs clippy with `-D warnings`, so every
  warning is an error. Every pub item needs a doc comment.
- `clippy::must_use_candidate` fires on pub functions returning a value; add
  `#[must_use]` (recurring trip-up, including in tests).
- `clippy::too_many_lines` fires above 100 lines; the fixture builder carries
  `#[allow(...)]`.
- `unsafe_code = "deny"`. The only sanctioned `unsafe` is the
  `env::set_var("APT_CONFIG", ...)` in tests/fixture.rs, which must run before
  the first libapt cache init. Do not add `unsafe` elsewhere; stdout handling
  must stay panic-free without it.
- Commit messages: Conventional Commits, lowercase, imperative, <= 50 chars.

## Architecture

- Thin binary (src/main.rs) over a lib split into: `apt` (single cache scan),
  `repository` (identity from libapt PackageFile metadata, `--repo` selector
  resolution), `query` (rows), `output` (human tables + JSON), `cli`.
- Provenance is exact `(name, version, architecture)` matching taken from
  libapt's own `VersionFile -> PackageFile` relation. Never re-implement index
  parsing, never call `Cache::update()`, never take locks: the tool is
  strictly read-only. oma-apt is the only APT interface.
- Repository identity is the base URI (`IndexFile::ArchiveURI("")`), grouped
  by normalized URI (trailing slash and scheme/host case ignored). Suite,
  codename, origin, label are NOT unique identifiers.

## Output invariants (keep them uniform)

- Human package tables always emit PACKAGE/VERSION/ARCH/REPOSITORY; `--repo`
  narrows rows, never columns. `--repos` is one row per (repository, suite)
  plus a PACKAGES column (distinct package names per suite, deduplicated per
  (uri, suite) key - not per index id, or `Architecture: all` packages
  overcount).
- JSON has exactly two envelopes: `{packages: [...]}` (rows carry
  `repositories` + `installed`) and, with `--repo`,
  `{repository: ..., packages: [...]}` (rows omit the redundant
  `repositories`). Single-package queries use the same envelope.
- Write output through `write_stdout` in main.rs, never `println!`/`print!`:
  they panic on EPIPE. Closed pipes exit quietly with 141
  (`AptListsError::Output` + `ErrorKind::BrokenPipe`).
- serde_json has no `preserve_order`: object keys print alphabetically, so
  example transcripts in README.md and docs/example-output.md show sorted
  keys.

## Testing

- Tests never touch the host APT state: `Fixture::build` creates an isolated
  APT root (own sources.list, package lists, dpkg status, extended_states)
  and points libapt at it via `APT_CONFIG`. Library-level tests must reuse
  the shared fixture (APT_CONFIG is process-global and set once by a
  OnceLock); CLI tests spawn the real binary with per-fixture env.
- `APT_CONFIG` does not persist across shell invocations when testing the
  binary manually, and libapt caches pkgcache.bin under the fixture's cache
  dir - wipe it after editing fixture list files.
- Fixture gotchas: dpkg status stanzas must look like complete dpkg entries
  (e.g. include `Installed-Size`), or libapt will not merge the installed
  version with the repository version and every `-i` row shows `-` as
  repository. Lists filenames must match apt's URItoFileName scheme (scheme
  dropped, `/` -> `_`); a wrong prefix silently drops the repository.
- The broken-pipe test relies on output exceeding the 64 KiB pipe buffer
  (5000 synthetic packages via `Fixture::build_with_synthetic_packages`);
  keep that guarantee if touching it.
- Package iteration order is alphabetical (`PackageSort::names()`); code
  such as the name-grouped counting in `apt::scan` (`chunk_by`) depends on
  it.
