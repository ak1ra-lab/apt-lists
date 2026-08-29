//! Typed errors for `apt-lists`.
//!
//! The tool deliberately distinguishes between "the repository exists but
//! nothing matched" (which is a successful, empty query) and errors such as
//! "the repository selector does not match any known repository" or "the
//! selector is ambiguous", so that a valid-looking selector can never silently
//! produce an empty result.

use std::fmt;

/// Operational errors; every variant is reported to the user verbatim.
#[derive(Debug)]
pub enum AptListsError {
    /// `libapt-pkg` failed to initialize or open the package cache.
    CacheInit(String),
    /// The APT cache contains no downloadable package indexes at all
    /// (e.g. `apt update` was never run).
    NoPackageIndexes,
    /// The `--repo` selector does not match any repository known to the cache.
    RepoNotFound {
        /// The selector as given by the user.
        selector: String,
        /// URIs of the repositories that do exist, for suggestions.
        known: Vec<String>,
    },
    /// A hostname selector matches several distinct repository URIs.
    /// The user must disambiguate by passing a full URI.
    AmbiguousRepoSelector {
        /// The selector as given by the user.
        selector: String,
        /// The distinct repository URIs the selector matched.
        candidates: Vec<String>,
    },
    /// The requested package does not exist in the APT cache.
    PackageNotFound(String),
    /// Writing to stdout failed. A closed pipe (`apt-lists ... | head`) is
    /// reported as [`std::io::ErrorKind::BrokenPipe`] and handled specially
    /// by the binary instead of being shown to the user.
    Output(std::io::Error),
}

impl fmt::Display for AptListsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AptListsError::CacheInit(msg) => {
                write!(f, "failed to initialize the APT cache: {msg}")
            }
            AptListsError::NoPackageIndexes => write!(
                f,
                "no APT package indexes are available; run `apt update` first \
                 (this tool never downloads package lists by itself)"
            ),
            AptListsError::RepoNotFound { selector, known } => {
                write!(f, "repository '{selector}' was not found in the APT cache")?;
                if known.is_empty() {
                    return Ok(());
                }
                writeln!(f, "\nrepositories known to the cache:")?;
                for uri in known {
                    writeln!(f, "  {uri}")?;
                }
                Ok(())
            }
            AptListsError::AmbiguousRepoSelector {
                selector,
                candidates,
            } => {
                writeln!(
                    f,
                    "repository selector '{selector}' is ambiguous; it matches \
                     multiple repository URIs:"
                )?;
                for uri in candidates {
                    writeln!(f, "  {uri}")?;
                }
                write!(f, "pass the full repository URI to disambiguate")
            }
            AptListsError::PackageNotFound(name) => {
                write!(f, "package '{name}' was not found in the APT cache")
            }
            AptListsError::Output(e) => write!(f, "failed to write to stdout: {e}"),
        }
    }
}

impl std::error::Error for AptListsError {}

impl From<std::io::Error> for AptListsError {
    fn from(e: std::io::Error) -> Self {
        AptListsError::Output(e)
    }
}
