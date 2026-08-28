//! User-facing repository selector resolution (`--repo`).

use crate::error::AptListsError;
use crate::repository::{normalize_uri, strip_scheme, RepoCatalog, Repository};

/// A successfully resolved `--repo` selector: exactly one repository plus the
/// set of index ids that belong to it.
#[derive(Debug, Clone)]
pub struct ResolvedRepo {
    /// The matched repository.
    pub repository: Repository,
    /// Ids ([`crate::repository::RepoIndex::id`]) of all package indexes of
    /// the repository.
    pub index_ids: Vec<usize>,
}

impl ResolvedRepo {
    /// `true` when the given index id belongs to the resolved repository.
    #[must_use]
    pub fn contains(&self, index_id: usize) -> bool {
        self.index_ids.contains(&index_id)
    }
}

/// Resolve a selector against the catalog (see [`RepoCatalog::resolve`]).
pub(super) fn resolve(
    catalog: &RepoCatalog,
    selector: &str,
) -> Result<ResolvedRepo, AptListsError> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Err(AptListsError::RepoNotFound {
            selector: selector.to_string(),
            known: catalog.known_uris(),
        });
    }

    let matched: Vec<&Repository> = if selector.contains("://") {
        let want = normalize_uri(selector);
        catalog
            .repositories()
            .iter()
            .filter(|r| normalize_uri(&r.uri) == want)
            .collect()
    } else {
        // Scheme-less: match by site (hostname) or by the URI minus its
        // scheme (hostname + path), both case-insensitively.
        let want = strip_scheme(selector);
        catalog
            .repositories()
            .iter()
            .filter(|r| {
                r.site
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(selector))
                    || strip_scheme(&r.uri).eq_ignore_ascii_case(&want)
            })
            .collect()
    };

    if matched.is_empty() {
        return Err(AptListsError::RepoNotFound {
            selector: selector.to_string(),
            known: catalog.known_uris(),
        });
    }

    // Hostname matching must never silently pick one of several distinct
    // repositories that share the same host. Require a full URI instead.
    let mut uris: Vec<String> = matched.iter().map(|r| r.uri.clone()).collect();
    uris.sort();
    uris.dedup();
    if uris.len() > 1 {
        return Err(AptListsError::AmbiguousRepoSelector {
            selector: selector.to_string(),
            candidates: uris,
        });
    }

    let repository = matched[0].clone();
    Ok(ResolvedRepo {
        index_ids: repository.indexes.iter().map(|i| i.id).collect(),
        repository,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::repository::{RepoCatalog, RepoIndex};

    fn idx(id: usize, uri: &str, site: Option<&str>) -> RepoIndex {
        RepoIndex {
            id,
            uri: uri.to_string(),
            site: site.map(str::to_string),
            archive: Some("trixie".to_string()),
            codename: Some("trixie".to_string()),
            origin: Some("Origin".to_string()),
            label: Some("Label".to_string()),
            component: Some("main".to_string()),
            arch: Some("amd64".to_string()),
            filename: format!("/lists/{id}_Packages"),
            index_type: "Debian Package Index".to_string(),
        }
    }

    fn catalog() -> RepoCatalog {
        let mut indexes = BTreeMap::new();
        indexes.insert(
            1,
            idx(1, "https://deb.debian.org/debian/", Some("deb.debian.org")),
        );
        indexes.insert(
            2,
            idx(
                2,
                "https://deb.debian.org/debian-security",
                Some("deb.debian.org"),
            ),
        );
        indexes.insert(
            3,
            idx(
                3,
                "https://deb.debian.org/debian-updates",
                Some("deb.debian.org"),
            ),
        );
        indexes.insert(
            4,
            idx(
                4,
                "https://security.debian.org/debian-security",
                Some("security.debian.org"),
            ),
        );
        RepoCatalog::from_indexes(&indexes)
    }

    #[test]
    fn uri_matches_ignoring_trailing_slash() {
        let c = catalog();
        let r = c.resolve("https://deb.debian.org/debian-security").unwrap();
        assert_eq!(r.repository.uri, "https://deb.debian.org/debian-security");
        assert_eq!(r.index_ids, vec![2]);

        let r = c.resolve("https://deb.debian.org/debian").unwrap();
        assert_eq!(r.repository.uri, "https://deb.debian.org/debian/");
        assert_eq!(r.index_ids, vec![1]);
    }

    #[test]
    fn uri_matching_is_case_insensitive_for_scheme_and_host() {
        let c = catalog();
        let r = c.resolve("HTTPS://Deb.Debian.ORG/debian-security").unwrap();
        assert_eq!(r.index_ids, vec![2]);
    }

    #[test]
    fn hostname_matches_site() {
        let c = catalog();
        let r = c.resolve("security.debian.org").unwrap();
        assert_eq!(r.index_ids, vec![4]);
    }

    #[test]
    fn hostname_with_path_matches_uri_without_scheme() {
        let c = catalog();
        let r = c.resolve("deb.debian.org/debian-updates").unwrap();
        assert_eq!(r.index_ids, vec![3]);
    }

    #[test]
    fn unknown_selector_is_an_error() {
        let c = catalog();
        let err = c.resolve("https://nonexistent.example/repo").unwrap_err();
        assert!(matches!(err, AptListsError::RepoNotFound { .. }));
    }

    #[test]
    fn empty_selector_is_an_error() {
        let c = catalog();
        let err = c.resolve("   ").unwrap_err();
        assert!(matches!(err, AptListsError::RepoNotFound { .. }));
    }

    #[test]
    fn ambiguous_hostname_is_an_error() {
        let c = catalog();
        let err = c.resolve("deb.debian.org").unwrap_err();
        match err {
            AptListsError::AmbiguousRepoSelector {
                selector,
                candidates,
            } => {
                assert_eq!(selector, "deb.debian.org");
                assert_eq!(
                    candidates,
                    vec![
                        "https://deb.debian.org/debian-security",
                        "https://deb.debian.org/debian-updates",
                        "https://deb.debian.org/debian/",
                    ]
                );
            }
            other => panic!("expected ambiguity error, got: {other:?}"),
        }
    }

    #[test]
    fn ambiguous_host_is_resolvable_by_path() {
        let c = catalog();
        let r = c.resolve("deb.debian.org/debian-updates").unwrap();
        assert_eq!(r.index_ids, vec![3]);
    }

    #[test]
    fn catalog_groups_by_normalized_uri() {
        let mut indexes = BTreeMap::new();
        indexes.insert(
            1,
            idx(1, "https://host.example/repo/", Some("host.example")),
        );
        indexes.insert(2, idx(2, "https://host.example/repo", Some("host.example")));
        let c = RepoCatalog::from_indexes(&indexes);
        // Same repository, two indexes.
        assert_eq!(c.repositories().len(), 1);
        assert_eq!(c.repositories()[0].indexes.len(), 2);
    }
}
