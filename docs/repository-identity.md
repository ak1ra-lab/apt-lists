# Repository identity and selectors

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

## Selectors (`-r` / `--repo`)

A selector may be one of:

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
