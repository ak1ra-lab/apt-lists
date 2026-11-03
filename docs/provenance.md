# Provenance and matching semantics

## What "provided by" means

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

This is also why `apt-lists` never guesses from package names alone. A
repository can provide `foo` while providing a different version than the one
installed; that repository is not a provider of the installed package.

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
