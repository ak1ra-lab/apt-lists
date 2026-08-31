# apt-lists

A small, read-only, repository-aware companion to `apt list`.

`apt-lists` answers one question that `apt list` cannot:

> Which installed package versions are currently provided by a specific APT
> repository?

```console
$ apt-lists --installed --repo https://deb.debian.org/debian-security/
PACKAGE  VERSION        ARCH   REPOSITORY
libbaz   3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/
seconly  2.0            all    https://deb.debian.org/debian-security/
```

It is **not** a repository management tool. It never downloads package
lists, installs or removes packages, and never modifies APT configuration or
the dpkg database. It only reads the state that `libapt-pkg` already has.

A full annotated transcript is available in
[`docs/example-output.md`](docs/example-output.md).

## Provenance semantics (read this first)

`apt-lists` determines that:

> the currently installed version of a package is **available from** repository
> X **according to the current local APT cache**.

It does **not** determine — and APT/dpkg cannot determine — that:

> the package was **originally installed from** repository X.

APT and dpkg do not record the installation source in the package status
database. If `foo 2.0` is currently in the cache from the archive and
`foo 1.0` from an older pocket, a system with `foo 2.0` installed will report
the archive — even if the package originally came from somewhere else and was
later upgraded. Treat all output as a statement about *current cache state*,
not history.

## Matching semantics

A package is reported for `--repo` only when the **exact triple**

```text
installed package name + installed version + installed architecture
```

is provided by that repository, per libapt-pkg's own `VersionFile` records.
The tool never merely checks whether a repository provides a package *name*:

```text
archive:            foo 2.0-1           (candidate)
debian-updates:     foo 2.0-1+deb13u1   (candidate — newer)
Installed:          foo 2.0-1

$ apt-lists --installed --repo https://deb.debian.org/debian-updates/
-> foo is NOT reported (the exact installed version is not in the pocket)

$ apt-lists --installed --repo https://deb.debian.org/debian/
-> foo IS reported
```

Architecture is part of the identity: a version with `Architecture: all` is
reported as `all`; `foo:amd64` and `foo:i386` are independent.

A version provided by several repositories is reported with **all** of them
(no candidate-priority filtering). A version that no repository in the cache
provides (e.g. a locally installed `.deb`) is shown with an empty repository
list in `--installed`, and never matches a `--repo` filter.

## How repository identity works

Suite, Codename, Component, Origin and Label are **not** globally unique.
`apt-lists` therefore builds identity from what `libapt-pkg` itself keeps for
each package index (`pkgCache::PackageFile` + its linked
`pkgCache::ReleaseFile`, exposed by `oma-apt` as `PackageFile`):

| field        | source                                     | example                                        |
| ------------ | ------------------------------------------ | ---------------------------------------------- |
| `uri`        | `IndexFile::ArchiveURI("")` (`REPO_URI`)   | `https://deb.debian.org/debian-security/`      |
| `site`       | `PackageFile::site()` (hostname)           | `deb.debian.org`                               |
| `filename`   | `PackageFile::filename()` (local index)    | `.../lists/deb.debian.org_debian_..._Packages` |
| `archive`    | `PackageFile::archive()` (suite)           | `trixie-security`                              |
| `codename`   | `PackageFile::codename()`                  | `trixie`                                       |
| `origin`     | `PackageFile::origin()`                    | `Debian Security` — *not* related to the host  |
| `label`      | `PackageFile::label()`                     | `Debian Security`                              |
| `component`  | `PackageFile::component()`                 | `main`                                         |
| `arch`       | `PackageFile::arch()` (index architecture) | `amd64`                                        |

Repositories are grouped by their base URI (one source entry may serve
several suites, components and architectures — each combination is a separate
package index inside the same repository). Note that Origin and repository
hostname are distinct fields and are preserved as such.

`libapt-pkg` does not expose a lossless way to reconstruct the exact URI
spelling from the list file name (it is a quoted, lossy encoding), so the
values above are reported **raw**. The URI is the one libapt itself reports
and carries a trailing slash; matching normalizes trailing slashes and the
case of scheme/host.

### Selectors (`-r` / `--repo`)

* full URI: `--repo https://deb.debian.org/debian-security/` — exact URI match
* hostname: `--repo security.debian.org` — matches the site
* host + path: `--repo deb.debian.org/debian-security`

If a hostname selector matches **several distinct repository URIs**, the tool
fails with an ambiguity error listing the candidates instead of silently
choosing one. This is not an edge case on a real Debian system:
`deb.debian.org` hosts `/debian`, `/debian-security` and `/debian-updates`,
so a bare `--repo deb.debian.org` is refused and a path is required. Three
outcomes are distinguishable:

1. repository matched, packages matched → results
2. repository matched, no installed packages match → empty output, exit 0
3. selector matched no repository → explicit error, exit 1

## Usage

```console
apt-lists --installed                          # all installed packages + providers
apt-lists --installed --repo <REPOSITORY>      # installed from one repository
apt-lists --repos                              # repositories known to the cache (+ package counts)
apt-lists <package>                            # all versions of a package + repositories
apt-lists <package>:<arch>                     # e.g. apt-lists foo:i386
apt-lists --all                                # all available package versions (default)
apt-lists --manual-installed                   # like `apt list --manual-installed`
```

Short forms: `-i` (`--installed`), `-r` (`--repo`), `-R` (`--repos`),
`-a` (`--all`), `-m` (`--manual-installed`), `-j` (`--json`).

Add `--json` for machine-readable output. `--repo` composes with
`--installed`, `--all` and package queries.

### Shell completion

`apt-lists` can print completion scripts for your shell:

```console
apt-lists --generate-completion bash | sudo tee /usr/share/bash-completion/completions/apt-lists
apt-lists --generate-completion zsh  > "${fpath[1]}/_apt-lists"
apt-lists --generate-completion fish > ~/.config/fish/completions/apt-lists.fish
```

Supported shells: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

### JSON

Every package-listing mode uses the same two shapes, so scripts can rely on
a uniform field set:

* without `--repo`: `{ "packages": [ ... ] }`, every row carries its
  `repositories` array and the `installed` flag;
* with `--repo`: `{ "repository": {...}, "packages": [ ... ] }`, the selected
  repository is reported once at the top level, so the rows omit their
  (redundant) `repositories` array.

```console
$ apt-lists --installed --repo https://deb.debian.org/debian-security/ --json
```

```json
{
  "repository": {
    "uri": "https://deb.debian.org/debian-security/",
    "site": "deb.debian.org",
    "origin": "Debian Security",
    "label": "Debian Security",
    "suites": [
      {
        "archive": "trixie-security",
        "codename": "trixie",
        "components": ["main"],
        "architectures": ["amd64"]
      }
    ]
  },
  "packages": [
    {
      "name": "libbaz",
      "version": "3.1-2+deb13u1",
      "architecture": "amd64",
      "installed": true
    },
    {
      "name": "seconly",
      "version": "2.0",
      "architecture": "all",
      "installed": true
    }
  ]
}
```

Single-package queries (`apt-lists foo --json`) use the same `packages`
envelope as every other mode. With `--repos --json`, the per-suite detail
(suites with codename, components, architectures and the number of distinct
package names as `packages`) and the Release metadata (`origin`, `label`,
per-index `index_filename`/`index_type`) are reported in full. No canonical
URI reconstruction is promised: the values are the raw ones libapt-pkg
exposes.

## Requirements

* Debian (or derivative) with `dpkg`; not portable to non-dpkg distributions.
* `libapt-pkg` ≥ 6.0 (Debian 12+). Primary target: Debian 13 / APT 3.x /
  `libapt-pkg.so.7.0` (developed and tested against apt 3.0.3).
* Build: Rust 1.85+ and `libapt-pkg-dev` (for the headers).
* `oma-apt` 0.13 is the APT interface — there is no second package-index
  parser in this codebase.

## Building

```console
sudo apt install build-essential libapt-pkg-dev
cargo build --release
# binary: target/release/apt-lists
```

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
  suite so the table stays narrow.
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
* CI (`.github/workflows/ci.yaml`, GitHub Actions-compatible syntax for
  Forgejo/Gitea Actions) runs fmt, clippy and the test suite on every push
  and pull request.

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

## License

GPL-3.0-or-later, see [LICENSE](LICENSE). The `oma-apt` dependency uses the
same license.
