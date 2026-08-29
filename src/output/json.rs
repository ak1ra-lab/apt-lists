//! JSON output (serde).
//!
//! JSON preserves the raw values libapt-pkg exposes — including the local
//! index file name — and never claims a canonical reconstruction of the
//! source entry beyond the base URI that libapt itself reports.

use serde::Serialize;

use crate::apt::PackageCounts;
use crate::query::VersionRow;
use crate::repository::{RepoCatalog, RepoIndex, Repository};

/// Pretty-print a JSON value (the human default for `--json` output).
#[must_use]
pub fn to_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).expect("JSON value is always serializable")
}

/// One package index, verbatim (`Repository` column granularity).
#[derive(Debug, Clone, Serialize)]
pub struct RepoRef {
    /// Repository base URI as reported by libapt-pkg (trailing slash).
    pub uri: String,
    /// Hostname of the source URI, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// Archive/Suite (e.g. `trixie`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
    /// Codename (e.g. `trixie`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codename: Option<String>,
    /// Origin field from the Release file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// Label field from the Release file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Component (e.g. `main`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    /// Architecture of the index (e.g. `amd64`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
    /// Local index file path under `/var/lib/apt/lists`.
    pub index_filename: String,
    /// Index type string from libapt-pkg.
    pub index_type: String,
}

impl From<&RepoIndex> for RepoRef {
    fn from(idx: &RepoIndex) -> Self {
        RepoRef {
            uri: idx.uri.clone(),
            site: idx.site.clone(),
            archive: idx.archive.clone(),
            codename: idx.codename.clone(),
            origin: idx.origin.clone(),
            label: idx.label.clone(),
            component: idx.component.clone(),
            architecture: idx.arch.clone(),
            index_filename: idx.filename.clone(),
            index_type: idx.index_type.clone(),
        }
    }
}

/// A package version row.
#[derive(Debug, Clone, Serialize)]
pub struct JsonVersion {
    /// Package name without architecture qualifier.
    pub name: String,
    /// Version string.
    pub version: String,
    /// Architecture of the version.
    pub architecture: String,
    /// Whether this version is the currently installed one (dpkg state).
    pub installed: bool,
    /// Repositories providing exactly this version. Omitted when a `--repo`
    /// filter is active (the repository is reported once, at the top level).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repositories: Option<Vec<RepoRef>>,
}

fn json_version(r: &VersionRow, with_repositories: bool) -> JsonVersion {
    JsonVersion {
        name: r.name.clone(),
        version: r.version.clone(),
        architecture: r.arch.clone(),
        installed: r.installed,
        repositories: if with_repositories {
            Some(r.repositories.iter().map(RepoRef::from).collect())
        } else {
            None
        },
    }
}

/// `--installed --json`, `--all --json` and package queries (without
/// `--repo`): `{ "packages": [ { name, version, architecture, installed,
/// repositories } ] }`.
#[must_use]
pub fn versions(rows: &[VersionRow]) -> serde_json::Value {
    let packages: Vec<JsonVersion> = rows.iter().map(|r| json_version(r, true)).collect();
    serde_json::json!({ "packages": packages })
}

/// `--repo <repo> --json` for `--installed` / `--all` / package queries:
/// `{ "repository": {...}, "packages": [ { name, version, architecture,
/// installed } ] }`. The selected repository is reported once at the top
/// level, so the rows omit their (redundant) `repositories` arrays.
#[must_use]
pub fn for_repo(repository: &Repository, rows: &[VersionRow]) -> serde_json::Value {
    let packages: Vec<JsonVersion> = rows.iter().map(|r| json_version(r, false)).collect();
    serde_json::json!({
        "repository": repository_json(repository, None),
        "packages": packages,
    })
}

/// `--repos --json`: repositories grouped by URI, with per-suite detail
/// (including the number of distinct package names per suite).
#[must_use]
pub fn repos(catalog: &RepoCatalog, package_counts: &PackageCounts) -> serde_json::Value {
    let repositories: Vec<serde_json::Value> = catalog
        .repositories()
        .iter()
        .map(|r| repository_json(r, Some(package_counts)))
        .collect();
    serde_json::json!({ "repositories": repositories })
}

fn repository_json(repo: &Repository, package_counts: Option<&PackageCounts>) -> serde_json::Value {
    let suites: Vec<serde_json::Value> = repo
        .archives()
        .iter()
        .map(|archive| {
            let indexes: Vec<RepoIndex> = repo
                .indexes
                .iter()
                .filter(|i| i.archive.as_deref() == Some(archive.as_str()))
                .cloned()
                .collect();
            let mut suite = serde_json::json!({
                "archive": archive,
                "codename": first(&indexes, |i| i.codename.clone()),
                "components": distinct(&indexes, |i| i.component.clone()),
                "architectures": distinct(&indexes, |i| i.arch.clone()),
            });
            if let Some(counts) = package_counts {
                suite["packages"] =
                    crate::apt::suite_package_count(counts, &repo.uri, Some(archive)).into();
            }
            suite
        })
        .collect();

    serde_json::json!({
        "uri": repo.uri,
        "site": repo.site,
        "origin": first(&repo.indexes, |i| i.origin.clone()),
        "label": first(&repo.indexes, |i| i.label.clone()),
        "suites": suites,
    })
}

fn first(indexes: &[RepoIndex], get: impl Fn(&RepoIndex) -> Option<String>) -> Option<String> {
    indexes.iter().find_map(get)
}

fn distinct(indexes: &[RepoIndex], get: impl Fn(&RepoIndex) -> Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    for i in indexes {
        if let Some(v) = get(i) {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}
