//! High-level queries over the scanned cache.
//!
//! All queries answer the same underlying relation
//!
//! ```text
//! installed packages
//!     JOIN installed version's VersionFiles
//!     JOIN repository identity
//! ```
//!
//! built from the single in-memory scan, with exact `(name, version,
//! architecture)` matching provided by libapt-pkg itself: a version belongs to
//! a repository if and only if libapt-pkg records a `VersionFile` for it in
//! that repository's `PackageFile`.

use crate::apt::{CacheScan, ScannedVersion};
use crate::error::AptListsError;
use crate::repository::{RepoCatalog, RepoIndex, ResolvedRepo};

/// A package version together with the repositories that provide exactly this
/// version.
#[derive(Debug, Clone)]
pub struct VersionRow {
    /// Package name without architecture qualifier.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Architecture of the version (`amd64`, `all`, ...).
    pub arch: String,
    /// Whether this is the currently installed version.
    pub installed: bool,
    /// Whether the package is marked auto-installed
    /// (`/var/lib/apt/extended_states`); the inverse of
    /// `apt list --manual-installed` membership. Only meaningful for
    /// installed versions.
    pub auto_installed: bool,
    /// Repositories providing exactly this version. May be empty when the
    /// version is not available from any repository in the cache (e.g. a
    /// locally installed .deb).
    pub repositories: Vec<RepoIndex>,
}

/// Filter which versions a query returns.
#[derive(Debug, Clone, Copy)]
pub enum RepoFilter<'a> {
    /// No repository filter; all providers are reported.
    None,
    /// Only versions provided by the resolved repository.
    Selected(&'a ResolvedRepo),
}

/// Extra constraints applied to installed-version queries.
#[derive(Debug, Clone, Copy, Default)]
pub struct InstalledFilter {
    /// Keep only packages that are *not* marked auto-installed, like
    /// `apt list --manual-installed`.
    pub manual_only: bool,
}

impl RepoFilter<'_> {
    fn matches(self, index_ids: &[usize]) -> bool {
        match self {
            RepoFilter::None => true,
            RepoFilter::Selected(sel) => index_ids.iter().any(|id| sel.contains(*id)),
        }
    }
}

fn resolve_repositories(
    catalog: &RepoCatalog,
    index_ids: &[usize],
    filter: RepoFilter<'_>,
) -> Vec<RepoIndex> {
    match filter {
        RepoFilter::None => index_ids
            .iter()
            .filter_map(|id| catalog.index_by_id(*id))
            .cloned()
            .collect(),
        RepoFilter::Selected(sel) => sel
            .repository
            .indexes
            .iter()
            .filter(|i| index_ids.contains(&i.id))
            .cloned()
            .collect(),
    }
}

/// All currently installed package versions.
///
/// Uses the *installed* version (dpkg state) of every package record, never
/// the candidate or depcache "install" version. When `filter` selects a
/// repository, only packages whose exact installed version is provided by
/// that repository are returned; their repository list is then exactly the
/// matching indexes of that repository. `installed_filter` can restrict the
/// result to manually installed packages (`apt list --manual-installed`).
#[must_use]
pub fn installed(
    scan: &CacheScan,
    catalog: &RepoCatalog,
    filter: RepoFilter<'_>,
    installed_filter: InstalledFilter,
) -> Vec<VersionRow> {
    let mut rows = Vec::new();

    for (pkg, ver) in scan.installed_versions() {
        if installed_filter.manual_only && pkg.auto_installed {
            continue;
        }
        if !filter.matches(&ver.index_ids) {
            continue;
        }
        rows.push(VersionRow {
            name: pkg.name.clone(),
            version: ver.version.clone(),
            arch: ver.arch.clone(),
            installed: true,
            auto_installed: pkg.auto_installed,
            repositories: resolve_repositories(catalog, &ver.index_ids, filter),
        });
    }

    rows
}

/// All versions of all packages known to the cache (`--all`).
#[must_use]
pub fn all_versions(
    scan: &CacheScan,
    catalog: &RepoCatalog,
    filter: RepoFilter<'_>,
) -> Vec<VersionRow> {
    let mut rows = Vec::new();

    for pkg in &scan.packages {
        for ver in &pkg.versions {
            if !filter.matches(&ver.index_ids) {
                continue;
            }
            rows.push(row_for(pkg, ver, catalog, filter));
        }
    }

    rows
}

/// All versions of a single package (`apt-lists <package>`).
///
/// Errors only when the package (or the requested `name:architecture`
/// combination) does not exist in the cache at all, so that an invalid
/// request is never confused with an empty result. A package that exists but
/// simply has no versions from the selected repository yields an empty list.
pub fn package_versions(
    scan: &CacheScan,
    catalog: &RepoCatalog,
    name: &str,
    filter: RepoFilter<'_>,
) -> Result<Vec<VersionRow>, AptListsError> {
    if scan.packages_by_name(name).next().is_none() {
        return Err(AptListsError::PackageNotFound(name.to_string()));
    }

    let mut rows = Vec::new();

    for pkg in scan.packages_by_name(name) {
        for ver in &pkg.versions {
            if !filter.matches(&ver.index_ids) {
                continue;
            }
            rows.push(row_for(pkg, ver, catalog, filter));
        }
    }

    Ok(rows)
}

fn row_for(
    pkg: &crate::apt::ScannedPackage,
    ver: &ScannedVersion,
    catalog: &RepoCatalog,
    filter: RepoFilter<'_>,
) -> VersionRow {
    VersionRow {
        name: pkg.name.clone(),
        version: ver.version.clone(),
        arch: ver.arch.clone(),
        installed: ver.installed,
        auto_installed: pkg.auto_installed,
        repositories: resolve_repositories(catalog, &ver.index_ids, filter),
    }
}
