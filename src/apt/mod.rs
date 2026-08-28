//! APT cache initialization and package/version discovery.
//!
//! Everything in this module is a thin layer over `oma-apt` (which binds to
//! `libapt-pkg`). No APT index files are parsed by hand: the exact relation
//!
//! ```text
//! Package -> installed Version -> VersionFile -> PackageFile
//! ```
//!
//! is taken from `libapt-pkg`'s own cache, exactly as `apt list` sees it.
//!
//! The scan is strictly read-only: the cache is only ever *built and read*,
//! never updated (`Cache::update()` is not called), no packages are marked,
//! resolved or installed, and no APT/dpkg state is modified.

use std::collections::BTreeMap;

use oma_apt::cache::{Cache, PackageSort};

use crate::error::AptListsError;
use crate::repository::RepoIndex;

/// A single available (or installed) version of a package.
#[derive(Debug, Clone)]
pub struct ScannedVersion {
    /// Version string, e.g. `1.2.3-1`.
    pub version: String,
    /// Architecture of the version itself, e.g. `amd64` or `all`.
    pub arch: String,
    /// Whether this version is the currently installed one (dpkg state).
    pub installed: bool,
    /// IDs ([`RepoIndex::id`]) of the downloadable package indexes that carry
    /// exactly this version. This is the exact `Package -> Version ->
    /// VersionFile -> PackageFile` relation; empty if no repository in the
    /// cache provides the version (e.g. locally installed .deb).
    pub index_ids: Vec<usize>,
}

/// One package record (a `(name, arch)` pair in dpkg terms) with all of its
/// known versions.
#[derive(Debug, Clone)]
pub struct ScannedPackage {
    /// Package name without architecture qualifier.
    pub name: String,
    /// Architecture of the package record (`amd64`, `i386`, `all`, ...).
    pub arch: String,
    /// Whether the package is marked as automatically installed
    /// (`/var/lib/apt/extended_states`), as used by
    /// `apt list --manual-installed`. Only meaningful when the package has an
    /// installed version.
    pub auto_installed: bool,
    /// All versions known to the cache, newest first (libapt order).
    pub versions: Vec<ScannedVersion>,
}

impl ScannedPackage {
    /// The version that is currently installed, if any.
    ///
    /// This always uses the *installed* version (dpkg state) and never the
    /// candidate or the "install" version from the depcache.
    #[must_use]
    pub fn installed_version(&self) -> Option<&ScannedVersion> {
        self.versions.iter().find(|v| v.installed)
    }
}

/// Result of a single pass over the whole APT cache.
pub struct CacheScan {
    /// All downloadable package indexes (repositories) that provide at least
    /// one package version, keyed by their unique in-cache id.
    pub indexes: BTreeMap<usize, RepoIndex>,
    /// All package records that have at least one version.
    pub packages: Vec<ScannedPackage>,
}

impl CacheScan {
    /// All package records with the given name.
    ///
    /// A package name can have several records (`foo:amd64`, `foo:all`, ...);
    /// each record carries the versions of its own architecture.
    pub fn packages_by_name<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Iterator<Item = &'a ScannedPackage> {
        let (name, arch) = match name.split_once(':') {
            Some((n, a)) => (n, Some(a)),
            None => (name, None),
        };

        self.packages
            .iter()
            .filter(move |p| p.name == name && p.has_version_arch(arch))
    }

    /// All installed versions across all packages.
    pub fn installed_versions(&self) -> impl Iterator<Item = (&ScannedPackage, &ScannedVersion)> {
        self.packages
            .iter()
            .filter_map(|p| p.installed_version().map(|v| (p, v)))
    }
}

impl ScannedPackage {
    fn has_version_arch(&self, arch: Option<&str>) -> bool {
        match arch {
            None => true,
            Some(a) => self.versions.iter().any(|v| v.arch == a),
        }
    }
}

/// Open the system APT cache through `oma-apt`/`libapt-pkg`.
///
/// This respects the system configuration (`/etc/apt/sources.list`,
/// `sources.list.d`, `APT_CONFIG`, ...) and the dpkg status database. It never
/// downloads anything and never writes to system state.
pub fn open_cache() -> Result<Cache, AptListsError> {
    oma_apt::new_cache!().map_err(|e| AptListsError::CacheInit(e.to_string()))
}

/// Walk the whole cache once and collect, in memory:
///
/// 1. the repository catalog (every downloadable `PackageFile`),
/// 2. every package version with the ids of the repositories providing it,
/// 3. which of those versions is the installed one, and whether the package
///    is marked auto-installed.
///
/// No external processes are spawned and no APT files are read manually.
pub fn scan(cache: &Cache) -> CacheScan {
    let mut indexes: BTreeMap<usize, RepoIndex> = BTreeMap::new();
    let mut packages: Vec<ScannedPackage> = Vec::new();

    let sort = PackageSort::default().names();
    for pkg in cache.packages(&sort) {
        let mut versions: Vec<ScannedVersion> = Vec::new();

        for ver in pkg.versions() {
            let mut index_ids: Vec<usize> = Vec::new();

            for vfile in ver.version_files() {
                let pfile = vfile.package_file();

                // The dpkg status database also shows up as a PackageFile
                // ("Debian dpkg status file"); it is not a repository and is
                // never downloadable. Skip it and anything else that cannot
                // be fetched from a repository.
                if !pfile.is_downloadable() {
                    continue;
                }

                let id = pfile.index();
                indexes
                    .entry(id)
                    .or_insert_with(|| RepoIndex::from_package_file(&pfile));
                index_ids.push(id);
            }

            index_ids.sort_unstable();
            index_ids.dedup();

            versions.push(ScannedVersion {
                version: ver.version().to_string(),
                arch: ver.arch().to_string(),
                installed: ver.is_installed(),
                index_ids,
            });
        }

        if versions.is_empty() {
            continue;
        }

        packages.push(ScannedPackage {
            name: pkg.name().to_string(),
            arch: pkg.arch().to_string(),
            auto_installed: pkg.is_auto_installed(),
            versions,
        });
    }

    CacheScan { indexes, packages }
}
