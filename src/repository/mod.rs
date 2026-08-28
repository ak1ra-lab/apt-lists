//! Repository identity and the `PackageFile -> repository` mapping.
//!
//! APT identifies a repository by more than one field, and none of Suite,
//! Codename, Component, Origin or Label is globally unique
//! (`deb.debian.org/debian trixie main` and
//! `deb.debian.org/debian-updates trixie-updates main` differ in their URI;
//! `deb.debian.org/debian trixie main` and a mirror of the same suite share
//! suite and component but are different repositories).
//!
//! `apt-lists` therefore builds repository identity from the data that
//! `libapt-pkg` itself keeps for each package index
//! (`pkgCache::PackageFile` + its linked `pkgCache::ReleaseFile`), exposed by
//! `oma-apt` as `PackageFile`:
//!
//! * `uri`      — the repository base URI as configured in the source entry,
//!   obtained through `IndexFile::ArchiveURI("")` (the `REPO_URI`
//!   of the index target). Two indexes share a URI exactly when
//!   they come from the same source entry.
//! * `site`     — the hostname of the source URI (`ReleaseFile::Site`), empty
//!   for e.g. `file://` sources whose URIs carry no host.
//! * `filename` — the local index file path under `/var/lib/apt/lists`,
//!   unique per (source entry, suite, component, architecture).
//! * `archive` / `codename` / `origin` / `label` / `component` / `arch` —
//!   taken verbatim from the Release/index metadata.
//!
//! libapt-pkg does not expose a way to reconstruct the source entry's exact
//! URI spelling (the lists file name is a lossy, quoted encoding of it), so
//! the values above are preserved raw and never "reverse engineered".

mod selector;

pub use selector::ResolvedRepo;

use std::collections::BTreeMap;

use oma_apt::PackageFile;

use crate::error::AptListsError;

/// Metadata of a single package index (one `Packages` file), i.e. the finest
/// granularity of "where a version comes from".
///
/// A source entry such as `deb https://deb.debian.org/debian trixie main`
/// yields one index per component and architecture.
#[derive(Debug, Clone)]
pub struct RepoIndex {
    /// Unique id of this index inside the current cache
    /// (`pkgCache::PkgFileIterator::Index()`).
    pub id: usize,
    /// Repository base URI as configured in the source entry, verbatim from
    /// libapt-pkg (`IndexFile::ArchiveURI("")`). Example:
    /// `https://deb.debian.org/debian-security/`.
    pub uri: String,
    /// Hostname of the source URI, if any (`PackageFile::site()`).
    pub site: Option<String>,
    /// Archive/Suite (e.g. `trixie`, `trixie-updates`).
    pub archive: Option<String>,
    /// Codename (e.g. `trixie`).
    pub codename: Option<String>,
    /// Origin (e.g. `Debian`) — *not* the same field as the hostname.
    pub origin: Option<String>,
    /// Label (e.g. `Debian`).
    pub label: Option<String>,
    /// Component (e.g. `main`).
    pub component: Option<String>,
    /// Architecture of the index (e.g. `amd64`).
    pub arch: Option<String>,
    /// Local index file path (e.g.
    /// `/var/lib/apt/lists/deb.debian.org_debian_dists_trixie_main_binary-amd64_Packages`).
    pub filename: String,
    /// Index type string from libapt-pkg (e.g. `Debian Package Index`).
    pub index_type: String,
}

impl RepoIndex {
    /// Extract repository metadata from a downloadable `PackageFile`.
    pub(crate) fn from_package_file(pfile: &PackageFile<'_>) -> RepoIndex {
        // `IndexFile::ArchiveURI("")` is the base URI of the repository as
        // configured in the source entry (the `REPO_URI` of the index target).
        let uri = pfile.index_file().archive_uri("");

        RepoIndex {
            id: pfile.index(),
            site: pfile.site().map(str::to_string),
            archive: pfile.archive().map(str::to_string),
            codename: pfile.codename().map(str::to_string),
            origin: pfile.origin().map(str::to_string),
            label: pfile.label().map(str::to_string),
            component: pfile.component().map(str::to_string),
            arch: pfile.arch().map(str::to_string),
            filename: pfile.filename().unwrap_or_default().to_string(),
            index_type: pfile.index_type().unwrap_or_default().to_string(),
            uri,
        }
    }
}

/// All indexes of one repository (one source URI), grouped together.
///
/// One repository typically serves several suites, components and
/// architectures; each combination is a separate [`RepoIndex`].
#[derive(Debug, Clone)]
pub struct Repository {
    /// Base URI of the repository (representative, verbatim from APT).
    pub uri: String,
    /// Hostname of the repository, if the source URI carries one.
    pub site: Option<String>,
    /// The indexes (suite/component/architecture combinations) of this
    /// repository, sorted by their index file name.
    pub indexes: Vec<RepoIndex>,
}

impl Repository {
    /// Distinct archives (suites) served by this repository.
    #[must_use]
    pub fn archives(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.archive.clone())
    }

    /// Distinct codenames served by this repository.
    #[must_use]
    pub fn codenames(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.codename.clone())
    }

    /// Distinct components served by this repository.
    #[must_use]
    pub fn components(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.component.clone())
    }

    /// Distinct index architectures served by this repository.
    #[must_use]
    pub fn archs(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.arch.clone())
    }

    /// Distinct origins, if published in the Release file.
    #[must_use]
    pub fn origins(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.origin.clone())
    }

    /// Distinct labels, if published in the Release file.
    #[must_use]
    pub fn labels(&self) -> Vec<String> {
        distinct(&self.indexes, |i| i.label.clone())
    }
}

fn distinct(indexes: &[RepoIndex], get: impl Fn(&RepoIndex) -> Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    for idx in indexes {
        if let Some(v) = get(idx) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

/// In-memory catalog of all repositories seen in the APT cache.
#[derive(Debug, Clone, Default)]
pub struct RepoCatalog {
    repositories: Vec<Repository>,
}

impl RepoCatalog {
    /// Group the raw package indexes into repositories (by base URI).
    #[must_use]
    pub fn from_indexes(indexes: &BTreeMap<usize, RepoIndex>) -> RepoCatalog {
        let mut by_uri: BTreeMap<String, Repository> = BTreeMap::new();

        for idx in indexes.values() {
            let key = normalize_uri(&idx.uri);
            let repo = by_uri.entry(key).or_insert_with(|| Repository {
                uri: idx.uri.clone(),
                site: idx.site.clone(),
                indexes: Vec::new(),
            });
            if repo.site.is_none() {
                repo.site.clone_from(&idx.site);
            }
            repo.indexes.push(idx.clone());
        }

        let mut repositories: Vec<Repository> = by_uri.into_values().collect();
        for repo in &mut repositories {
            repo.indexes.sort_by(|a, b| a.filename.cmp(&b.filename));
        }
        repositories.sort_by(|a, b| a.uri.cmp(&b.uri));

        RepoCatalog { repositories }
    }

    /// The repositories of the catalog, sorted by URI.
    #[must_use]
    pub fn repositories(&self) -> &[Repository] {
        &self.repositories
    }

    /// `true` when the cache exposed no downloadable package index at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.repositories.is_empty()
    }

    /// URIs of all known repositories (sorted).
    #[must_use]
    pub fn known_uris(&self) -> Vec<String> {
        self.repositories.iter().map(|r| r.uri.clone()).collect()
    }

    /// Look up a single index by its in-cache id.
    #[must_use]
    pub fn index_by_id(&self, id: usize) -> Option<&RepoIndex> {
        self.repositories
            .iter()
            .flat_map(|r| r.indexes.iter())
            .find(|i| i.id == id)
    }

    /// Resolve a user-facing repository selector against the catalog.
    ///
    /// Supported forms:
    ///
    /// * a repository URI (`https://deb.debian.org/debian-security`) —
    ///   matched against
    ///   the repository base URI, ignoring a trailing slash; scheme and
    ///   host compare case-insensitively;
    /// * a hostname (`security.debian.org`) — matched against the site of the
    ///   repository; ambiguous when several distinct URIs share the host;
    /// * a hostname with a path (`deb.debian.org/debian-security`) —
    ///   matched against
    ///   the URI without its scheme.
    ///
    /// The failure modes are distinguishable: [`Ok`] means the selector
    /// matched exactly one repository, [`AptListsError::AmbiguousRepoSelector`]
    /// means the selector matched several distinct repository URIs, and
    /// [`AptListsError::RepoNotFound`] means nothing matched at all.
    pub fn resolve(&self, selector: &str) -> Result<ResolvedRepo, AptListsError> {
        selector::resolve(self, selector)
    }
}

/// Lower-case the scheme and host part of a URI, drop a trailing slash.
///
/// URI paths stay case-sensitive; only scheme and authority are normalized.
pub(crate) fn normalize_uri(uri: &str) -> String {
    let uri = uri.trim();
    let lowered = match uri.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = match rest.split_once('/') {
                Some((h, p)) => (h, Some(p)),
                None => (rest, None),
            };
            format!(
                "{}://{}{}",
                scheme.to_ascii_lowercase(),
                host.to_ascii_lowercase(),
                match path {
                    Some(p) => format!("/{p}"),
                    None => String::new(),
                }
            )
        }
        None => uri.to_string(),
    };
    lowered.trim_end_matches('/').to_string()
}

/// Drop the scheme (`http://`, `file://`, ...) from a URI-ish string.
pub(crate) fn strip_scheme(uri: &str) -> String {
    match uri.split_once("://") {
        Some((_, rest)) => rest.to_string(),
        None => uri.to_string(),
    }
    .trim_end_matches('/')
    .to_string()
}
