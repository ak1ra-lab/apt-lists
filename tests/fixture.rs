//! Integration tests for `apt-lists`.
//!
//! Strategy: the tests never touch the host system's APT state. Instead,
//! every fixture builds an isolated APT root (own `sources.list`, package
//! lists, dpkg status, extended states and cache directories) and points
//! `libapt-pkg` at it via the documented `APT_CONFIG` environment variable.
//!
//! Only official Debian repository URIs are used as fixtures:
//!
//! ```text
//! https://deb.debian.org/debian            trixie           (main archive)
//! https://ftp.us.debian.org/debian         trixie           (official mirror, same suite!)
//! https://deb.debian.org/debian-updates    trixie-updates
//! https://deb.debian.org/debian-security   trixie-security
//! ```
//!
//! Package fixture (component `main`, architectures amd64 + i386):
//!
//! ```text
//! deb.debian.org/debian      amd64: foo 2.0-1, libbaz 3.1-2, shared 5.0(all),
//!                                   debonly 1.0(all), secman 1.0(all)
//!                           i386:  foo 2.0-1
//! ftp.us.debian.org/debian   amd64: foo 2.0-1, libbaz 3.1-2      <- same exact
//!                                   versions as the main archive (mirror)
//! deb.debian.org/debian-updates   amd64: foo 2.0-1+deb13u1, updonly 3.0(all)
//! deb.debian.org/debian-security  amd64: libbaz 3.1-2+deb13u1, seconly 2.0(all)
//!
//! installed (dpkg status):          auto-installed (extended_states):
//!   foo 2.0-1 amd64                   shared
//!   foo 2.0-1 i386                    debonly
//!   shared 5.0 all
//!   debonly 1.0 all
//!   libbaz 3.1-2+deb13u1 amd64       <- the security update, NOT 3.1-2
//!   secman 1.0 all
//!   localonly 4.2 all                <- provided by nobody (status only)
//!   foreign 3.0 all                  <- config-files state, must not be listed
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use tempfile::TempDir;

use apt_lists::apt::{self, CacheScan};
use apt_lists::error::AptListsError;
use apt_lists::query::{self, InstalledFilter, RepoFilter};
use apt_lists::repository::RepoCatalog;

const DEBIAN: &str = "https://deb.debian.org/debian";
const MIRROR: &str = "https://ftp.us.debian.org/debian";
const UPDATES: &str = "https://deb.debian.org/debian-updates";
const SECURITY: &str = "https://deb.debian.org/debian-security";

/// Mirror of apt's `URItoFileName` for the simple fixture URIs:
/// scheme dropped, `/` replaced by `_`.
fn uri_to_prefix(uri: &str) -> String {
    uri.split_once("://")
        .expect("fixture uris have a scheme")
        .1
        .replace('/', "_")
}

fn stanza(name: &str, version: &str, arch: &str, description: &str) -> String {
    format!(
        "Package: {name}\n\
         Version: {version}\n\
         Architecture: {arch}\n\
         Maintainer: Fixture <fixture@example.invalid>\n\
         Installed-Size: 10\n\
         Description: {description}\n\
         Filename: pool/main/{}/{name}_{version}_{arch}.deb\n\
         Size: 100\n\
         SHA256: {}\n\n",
        name.chars().next().unwrap(),
        "0".repeat(64)
    )
}

fn status_stanza(name: &str, version: &str, arch: &str, status: &str) -> String {
    format!(
        "Package: {name}\n\
         Status: {status}\n\
         Version: {version}\n\
         Architecture: {arch}\n\
         Maintainer: Fixture <fixture@example.invalid>\n\
         Installed-Size: 10\n\
         Description: {name} {version} {arch}\n\n"
    )
}

fn release(origin: &str, suite: &str) -> String {
    format!(
        "Origin: {origin}\n\
         Label: {origin}\n\
         Suite: {suite}\n\
         Codename: trixie\n\
         Date: Thu, 01 Jan 2026 00:00:00 UTC\n\
         Architectures: amd64 i386\n\
         Components: main\n\
         Description: fixture release\n"
    )
}

/// An isolated APT root. The directory is removed when the fixture is dropped.
pub struct Fixture {
    root: PathBuf,
    _dir: TempDir,
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl Fixture {
    /// Build a fresh isolated APT root under the system temp dir.
    #[must_use]
    pub fn build(tag: &str) -> Fixture {
        Self::build_inner(tag, 0)
    }

    /// Build a fixture with `n` additional synthetic packages
    /// (`synth-NNNNN`, version 1.0, amd64, only in the main archive, not
    /// installed). Used to produce output larger than the OS pipe buffer in
    /// the broken-pipe test.
    #[must_use]
    pub fn build_with_synthetic_packages(tag: &str, n: usize) -> Fixture {
        Self::build_inner(tag, n)
    }

    // The builder is long because it is mostly fixture *data*.
    #[allow(clippy::too_many_lines)]
    fn build_inner(tag: &str, synthetic: usize) -> Fixture {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = tempfile::Builder::new()
            .prefix(&format!("apt-lists-test-{tag}-{n}-"))
            .tempdir()
            .expect("create fixture tempdir");
        let root = dir.path().to_path_buf();
        let root_str = root.display().to_string();

        for rel in [
            "etc/apt.conf.d",
            "state/lists",
            "dpkg",
            "cache",
            "etc/sources.list.d",
        ] {
            std::fs::create_dir_all(root.join(rel)).expect("create fixture dir");
        }

        let f = Fixture { root, _dir: dir };

        f.write(
            "etc/apt.conf",
            &format!(
                "Dir {{\n\
                 \x20 State \"{root_str}/state/\";\n\
                 \x20 State::status \"{root_str}/dpkg/status\";\n\
                 \x20 Cache \"{root_str}/cache/\";\n\
                 \x20 Etc \"{root_str}/etc/\";\n\
                 \x20 Etc::sourcelist \"{root_str}/etc/sources.list\";\n\
                 \x20 Etc::sourceparts \"{root_str}/etc/sources.list.d\";\n\
                 }};\n\
                 APT::Architectures {{ \"i386\"; }};\n"
            ),
        );

        f.write(
            "etc/sources.list",
            &format!(
                "deb {DEBIAN} trixie main\n\
                 deb {MIRROR} trixie main\n\
                 deb {UPDATES} trixie-updates main\n\
                 deb {SECURITY} trixie-security main\n"
            ),
        );

        // Package indexes.
        let mut debian_amd64 = vec![
            stanza("foo", "2.0-1", "amd64", "debian foo"),
            stanza("libbaz", "3.1-2", "amd64", "baz library"),
            stanza("shared", "5.0", "all", "shared everywhere"),
            stanza("debonly", "1.0", "all", "debian only"),
            stanza("secman", "1.0", "all", "manual fixture pkg"),
        ];
        debian_amd64.extend((0..synthetic).map(|i| {
            stanza(
                &format!("synth-{i:05}"),
                "1.0",
                "amd64",
                "synthetic package",
            )
        }));
        f.write_packages(DEBIAN, "trixie", "amd64", &debian_amd64);
        f.write_packages(
            DEBIAN,
            "trixie",
            "i386",
            &[stanza("foo", "2.0-1", "i386", "debian foo i386")],
        );
        f.write_packages(
            MIRROR,
            "trixie",
            "amd64",
            &[
                stanza("foo", "2.0-1", "amd64", "mirror foo"),
                stanza("libbaz", "3.1-2", "amd64", "mirror baz"),
                stanza("shared", "5.0", "all", "shared everywhere"),
            ],
        );
        f.write_packages(
            UPDATES,
            "trixie-updates",
            "amd64",
            &[
                stanza("foo", "2.0-1+deb13u1", "amd64", "updates foo"),
                stanza("updonly", "3.0", "all", "updates only"),
            ],
        );
        f.write_packages(
            SECURITY,
            "trixie-security",
            "amd64",
            &[
                stanza("libbaz", "3.1-2+deb13u1", "amd64", "baz security update"),
                stanza("seconly", "2.0", "all", "security only"),
            ],
        );

        // Release metadata for each repository.
        for (uri, origin, suite) in [
            (DEBIAN, "Debian", "trixie"),
            (MIRROR, "Debian", "trixie"),
            (UPDATES, "Debian", "trixie-updates"),
            (SECURITY, "Debian Security", "trixie-security"),
        ] {
            f.write(
                &format!("state/lists/{}_dists_{suite}_Release", uri_to_prefix(uri)),
                &release(origin, suite),
            );
        }

        // dpkg status database.
        let status = format!(
            "{}{}{}{}{}{}{}{}{}",
            status_stanza("foo", "2.0-1", "amd64", "install ok installed"),
            status_stanza("foo", "2.0-1", "i386", "install ok installed"),
            status_stanza("shared", "5.0", "all", "install ok installed"),
            status_stanza("debonly", "1.0", "all", "install ok installed"),
            status_stanza("libbaz", "3.1-2+deb13u1", "amd64", "install ok installed"),
            status_stanza("secman", "1.0", "all", "install ok installed"),
            status_stanza("seconly", "2.0", "all", "install ok installed"),
            status_stanza("localonly", "4.2", "all", "install ok installed"),
            status_stanza("foreign", "3.0", "all", "deinstall ok config-files"),
        );
        f.write("dpkg/status", &status);

        // Auto-install marks (read by the depcache from
        // Dir::State::extended_states): `shared` and `debonly` are automatic,
        // everything else counts as manually installed.
        f.write(
            "state/extended_states",
            "Package: shared\n\
             Architecture: all\n\
             Auto-Installed: 1\n\
             \n\
             Package: debonly\n\
             Architecture: all\n\
             Auto-Installed: 1\n\
             \n",
        );

        f
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent dir");
        }
        let mut file = std::fs::File::create(path).expect("create fixture file");
        file.write_all(content.as_bytes())
            .expect("write fixture file");
    }

    fn write_packages(&self, uri: &str, suite: &str, arch: &str, stanzas: &[String]) {
        self.write(
            &format!(
                "state/lists/{}_dists_{suite}_main_binary-{arch}_Packages",
                uri_to_prefix(uri)
            ),
            &stanzas.concat(),
        );
    }

    #[must_use]
    /// Absolute path of the fixture's `apt.conf` (for `APT_CONFIG`).
    pub fn apt_config(&self) -> PathBuf {
        self.root.join("etc/apt.conf")
    }

    #[must_use]
    /// Root directory of the fixture.
    pub fn path(&self) -> &Path {
        &self.root
    }
}

// ---------------------------------------------------------------------------
// Library-level fixture access
// ---------------------------------------------------------------------------

static LIB_FIXTURE: OnceLock<Fixture> = OnceLock::new();
static SCAN_LOCK: Mutex<()> = Mutex::new(());

/// The shared library-level fixture. `APT_CONFIG` must be set before the very
/// first libapt cache is created in this process, which the OnceLock
/// guarantees.
fn fixture() -> &'static Fixture {
    LIB_FIXTURE.get_or_init(|| {
        let f = Fixture::build("lib");
        // SAFETY: this runs exactly once, from the OnceLock initializer,
        // before any threads that could observe the environment exist. The
        // value is never modified afterwards, and libapt reads `APT_CONFIG`
        // only once during `pkgInitConfig`.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("APT_CONFIG", f.apt_config());
        }
        f
    })
}

/// Open the cache against the shared fixture and run one full scan.
/// Serialized so that concurrent tests do not race on pkgcache.bin.
fn scan() -> (CacheScan, RepoCatalog) {
    let _guard = SCAN_LOCK.lock().unwrap();
    fixture();
    let cache = apt::open_cache().expect("cache must open against the fixture");
    let scan = apt::scan(&cache);
    let catalog = RepoCatalog::from_indexes(&scan.indexes);
    (scan, catalog)
}

fn filter_none() -> RepoFilter<'static> {
    RepoFilter::None
}

fn installed_all() -> InstalledFilter {
    InstalledFilter::default()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn repositories_are_listed_and_distinct() {
    let (_scan, catalog) = scan();
    // apt's REPO_URI is normalized with a trailing slash; the raw value is
    // preserved throughout the tool.
    let slash = |uri: &str| format!("{uri}/");
    let uris = catalog.known_uris();
    assert_eq!(
        uris,
        vec![
            slash(SECURITY),
            slash(UPDATES),
            slash(DEBIAN),
            slash(MIRROR),
        ]
    );

    // deb.debian.org/debian and ftp.us.debian.org/debian share suite,
    // codename and component — they must still be distinct repositories.
    let debian = catalog.resolve(DEBIAN).unwrap();
    let mirror = catalog.resolve(MIRROR).unwrap();
    assert_ne!(debian.repository.uri, mirror.repository.uri);
    assert_eq!(debian.repository.archives(), mirror.repository.archives());
    assert_eq!(debian.repository.site.as_deref(), Some("deb.debian.org"));
    assert_eq!(mirror.repository.site.as_deref(), Some("ftp.us.debian.org"));

    // The security pocket carries its own origin.
    assert_eq!(
        catalog.resolve(SECURITY).unwrap().repository.origins(),
        vec!["Debian Security"]
    );
}

#[test]
fn installed_versions_match_exact_repositories() {
    let (scan, catalog) = scan();

    let rows = query::installed(&scan, &catalog, filter_none(), installed_all());
    let find = |name: &str, version: &str, arch: &str| {
        rows.iter()
            .find(|r| r.name == name && r.version == version && r.arch == arch)
            .unwrap_or_else(|| panic!("row {name} {version} {arch} missing"))
    };
    let uris = |r: &query::VersionRow| -> Vec<String> {
        r.repositories.iter().map(|i| i.uri.clone()).collect()
    };
    let deb = format!("{DEBIAN}/");
    let mirror = format!("{MIRROR}/");
    let security = format!("{SECURITY}/");

    // Exact version match: foo 2.0-1 amd64 is in the archive and its mirror.
    assert_eq!(
        uris(find("foo", "2.0-1", "amd64")),
        vec![deb.clone(), mirror.clone()]
    );
    // Architecture is part of the identity: foo 2.0-1 i386 only in debian.
    assert_eq!(uris(find("foo", "2.0-1", "i386")), vec![deb.clone()]);
    // Multiple providers for one exact version.
    assert_eq!(
        uris(find("shared", "5.0", "all")),
        vec![deb.clone(), mirror.clone()]
    );
    // The installed libbaz is the security update; neither the archive nor
    // the mirror provides that exact version.
    assert_eq!(
        uris(find("libbaz", "3.1-2+deb13u1", "amd64")),
        vec![security.clone()]
    );
    // Status-only package: installed, but no repository provides it.
    assert!(find("localonly", "4.2", "all").repositories.is_empty());
    // config-files state is not "installed".
    assert!(rows.iter().all(|r| r.name != "foreign"));
}

#[test]
fn candidate_version_is_never_used_for_installed_queries() {
    let (scan, catalog) = scan();

    // debian-updates holds foo 2.0-1+deb13u1, which is the candidate for foo,
    // but the installed version is 2.0-1. The updates pocket must therefore
    // not report foo; nothing else of it is installed either.
    let sel = catalog.resolve(UPDATES).unwrap();
    let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
    assert!(rows.is_empty());

    // The security pocket provides the installed libbaz and seconly, but not
    // the foo versions (only 2.0-1 and 2.0-1+deb13u1 exist elsewhere).
    let sel = catalog.resolve(SECURITY).unwrap();
    let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
    assert_eq!(rows.len(), 2);
    assert!(rows
        .iter()
        .any(|r| r.name == "libbaz" && r.version == "3.1-2+deb13u1" && r.arch == "amd64"));
    assert!(rows.iter().any(|r| r.name == "seconly" && r.arch == "all"));

    // The archive provides the installed foo and debonly/secman/shared, but
    // NOT the installed libbaz (its libbaz 3.1-2 differs from the installed
    // 3.1-2+deb13u1 security update).
    let sel = catalog.resolve(DEBIAN).unwrap();
    let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
    assert_eq!(rows.len(), 5);
    assert!(rows
        .iter()
        .any(|r| r.name == "foo" && r.arch == "amd64" && r.version == "2.0-1"));
    assert!(rows.iter().all(|r| r.name != "libbaz"));
}

#[test]
fn repo_filtering_reports_multiplicity() {
    let (scan, catalog) = scan();

    // foo 2.0-1 amd64 exists in two repositories; both are reported when no
    // filter is applied.
    let rows = query::installed(&scan, &catalog, filter_none(), installed_all());
    let foo = rows
        .iter()
        .find(|r| r.name == "foo" && r.arch == "amd64")
        .unwrap();
    assert_eq!(foo.repositories.len(), 2);

    // Filtering by each provider yields the same package with exactly that
    // one provider.
    for uri in [DEBIAN, MIRROR] {
        let sel = catalog.resolve(uri).unwrap();
        let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
        let foo = rows
            .iter()
            .find(|r| r.name == "foo" && r.arch == "amd64")
            .unwrap();
        assert_eq!(foo.repositories.len(), 1);
        assert_eq!(foo.repositories[0].uri, format!("{uri}/"));
    }
}

#[test]
fn manual_installed_filter() {
    let (scan, catalog) = scan();

    let manual = query::installed(
        &scan,
        &catalog,
        filter_none(),
        InstalledFilter { manual_only: true },
    );
    let manual_names: Vec<&str> = manual.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        manual_names,
        vec!["foo", "foo", "libbaz", "localonly", "secman", "seconly"]
    );

    // Without the filter everything installed is listed.
    let all = query::installed(&scan, &catalog, filter_none(), installed_all());
    assert_eq!(all.len(), manual.len() + 2); // + shared, debonly (auto)
}

#[test]
fn existing_repo_without_installed_matches_is_empty_ok() {
    let (scan, catalog) = scan();

    // debian-updates exists but none of its packages are installed: empty
    // result, not an error.
    let sel = catalog.resolve(UPDATES).unwrap();
    let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
    assert!(rows.is_empty());

    // ...while its package versions are visible in an unfiltered query.
    // The same (version, architecture) from two repositories would be one
    // version with two providers; updonly exists only in the updates pocket.
    let updonly = query::package_versions(&scan, &catalog, "updonly", filter_none()).unwrap();
    assert_eq!(updonly.len(), 1);
    assert!(updonly.iter().all(|r| !r.installed));
    assert_eq!(updonly[0].repositories[0].uri, format!("{UPDATES}/"));
}

#[test]
fn package_queries_cover_all_architectures() {
    let (scan, catalog) = scan();

    // Unqualified name: every architecture.
    let rows = query::package_versions(&scan, &catalog, "foo", filter_none()).unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .any(|r| r.arch == "amd64" && r.version == "2.0-1" && r.installed));
    assert!(rows
        .iter()
        .any(|r| r.arch == "i386" && r.version == "2.0-1" && r.installed));
    assert!(rows
        .iter()
        .any(|r| r.arch == "amd64" && r.version == "2.0-1+deb13u1" && !r.installed));

    // Qualified name restricts to that architecture.
    let rows = query::package_versions(&scan, &catalog, "foo:i386", filter_none()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].arch, "i386");

    // Architecture: all packages appear under their name.
    let rows = query::package_versions(&scan, &catalog, "shared", filter_none()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].arch, "all");
    assert_eq!(rows[0].repositories.len(), 2);

    // Unknown package is an explicit error.
    let err = query::package_versions(&scan, &catalog, "no-such-pkg", filter_none()).unwrap_err();
    assert!(matches!(err, AptListsError::PackageNotFound(_)));
}

#[test]
fn selector_resolution_edge_cases() {
    let (_scan, catalog) = scan();

    // URI with and without trailing slash.
    assert!(catalog.resolve(&format!("{SECURITY}/")).is_ok());

    // Hostname.
    let r = catalog.resolve("ftp.us.debian.org").unwrap();
    assert_eq!(r.repository.uri, format!("{MIRROR}/"));

    // Unknown repository: explicit error, not an empty result.
    let err = catalog
        .resolve("https://nonexistent.invalid/apt")
        .unwrap_err();
    match &err {
        AptListsError::RepoNotFound { selector, known } => {
            assert_eq!(selector, "https://nonexistent.invalid/apt");
            assert_eq!(known.len(), 4);
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // Three distinct URIs share deb.debian.org: ambiguous.
    let err = catalog.resolve("deb.debian.org").unwrap_err();
    match &err {
        AptListsError::AmbiguousRepoSelector {
            selector,
            candidates,
        } => {
            assert_eq!(selector, "deb.debian.org");
            assert_eq!(
                candidates,
                &vec![
                    format!("{SECURITY}/"),
                    format!("{UPDATES}/"),
                    format!("{DEBIAN}/"),
                ]
            );
        }
        other => panic!("unexpected error: {other:?}"),
    }

    // Host + path disambiguates.
    assert_eq!(
        catalog
            .resolve("deb.debian.org/debian-security")
            .unwrap()
            .repository
            .uri,
        format!("{SECURITY}/")
    );
}

#[test]
fn json_output_shapes() {
    let (scan, catalog) = scan();

    // --installed --repo X --json
    let sel = catalog.resolve(SECURITY).unwrap();
    let rows = query::installed(&scan, &catalog, RepoFilter::Selected(&sel), installed_all());
    let value = apt_lists::output::json::for_repo(&sel.repository, &rows);
    let obj = value.as_object().unwrap();
    assert!(obj.contains_key("repository") && obj.contains_key("packages"));
    assert_eq!(obj["repository"]["uri"], format!("{SECURITY}/"));
    assert_eq!(obj["repository"]["site"], "deb.debian.org");
    let packages = obj["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 2);
    for p in packages {
        assert!(
            p.get("repositories").is_none(),
            "repo filtered rows must not repeat repositories"
        );
        assert!(p["name"].is_string() && p["version"].is_string() && p["architecture"].is_string());
        assert_eq!(p["installed"], true, "installed flag is always present");
    }

    // --installed --json (no repo filter): repositories inline.
    let rows = query::installed(&scan, &catalog, filter_none(), installed_all());
    let value = apt_lists::output::json::versions(&rows);
    let packages = value["packages"].as_array().unwrap();
    assert_eq!(packages.len(), 8);
    let foo = packages
        .iter()
        .find(|p| p["name"] == "foo" && p["architecture"] == "amd64")
        .unwrap();
    assert_eq!(foo["repositories"].as_array().unwrap().len(), 2);
    assert_eq!(foo["repositories"][0]["index_type"], "Debian Package Index");
    assert_eq!(foo["installed"], true);

    // --all --json: versions that are not installed carry installed: false.
    let rows = query::all_versions(&scan, &catalog, filter_none());
    let value = apt_lists::output::json::versions(&rows);
    let updonly = value["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "updonly")
        .unwrap();
    assert_eq!(updonly["installed"], false);
    assert_eq!(updonly["repositories"][0]["uri"], format!("{UPDATES}/"));

    // --repos --json
    let value = apt_lists::output::json::repos(&catalog);
    let repos = value["repositories"].as_array().unwrap();
    assert_eq!(repos.len(), 4);
    let security = repos
        .iter()
        .find(|r| r["uri"] == format!("{SECURITY}/"))
        .unwrap();
    assert_eq!(security["suites"].as_array().unwrap().len(), 1);
    assert_eq!(security["suites"][0]["archive"], "trixie-security");
    assert_eq!(
        security["suites"][0]["architectures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["amd64"]
    );
}

// ---------------------------------------------------------------------------
// CLI-level tests (spawn the real binary against its own fixture)
// ---------------------------------------------------------------------------

fn run_cli(fixture: &Fixture, args: &[&str]) -> (String, String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_apt-lists"))
        .args(args)
        .env("APT_CONFIG", fixture.apt_config())
        .output()
        .expect("spawn apt-lists");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

#[test]
fn cli_repos_and_installed_and_repo_filtering() {
    let f = Fixture::build("cli");

    // --repos (and the -R short form) lists every repository URI.
    for flag in ["--repos", "-R"] {
        let (stdout, _stderr, code) = run_cli(&f, &[flag]);
        assert_eq!(code, Some(0), "flag: {flag}");
        for uri in [DEBIAN, MIRROR, UPDATES, SECURITY] {
            assert!(stdout.contains(uri), "missing {uri} in:\n{stdout}");
        }
        assert!(stdout.contains("ftp.us.debian.org"), "flag: {flag}");
    }

    // --installed --repo <security>: exact-version semantics, short forms.
    // The table shape is uniform: the REPOSITORY column is present even with
    // a --repo filter.
    let (stdout, _stderr, code) = run_cli(&f, &["-i", "-r", SECURITY]);
    assert_eq!(code, Some(0));
    assert_eq!(
        stdout,
        "PACKAGE  VERSION        ARCH   REPOSITORY\n\
         libbaz   3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/\n\
         seconly  2.0            all    https://deb.debian.org/debian-security/\n"
    );
    assert!(
        !stdout.contains("foo"),
        "the security pocket does not provide the installed foo:\n{stdout}"
    );

    // Hostname selector for the mirror pocket: foo 2.0-1 amd64 and shared
    // 5.0 all are installed from there; the installed libbaz is the security
    // update and seconly is not in the mirror at all.
    let (stdout, _stderr, code) = run_cli(&f, &["--installed", "--repo", "ftp.us.debian.org"]);
    assert_eq!(code, Some(0));
    assert!(
        stdout.contains("foo") && stdout.contains("shared"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("libbaz") && !stdout.contains("seconly"),
        "{stdout}"
    );

    // Package query shows all versions with repositories.
    let (stdout, _stderr, code) = run_cli(&f, &["foo"]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("2.0-1") && stdout.contains("2.0-1+deb13u1"));
    assert!(stdout.contains("i386"));
    assert!(stdout.contains("[installed]"));

    // Single-package JSON uses the same envelope and row fields as the
    // other no-repo modes.
    let (stdout, _stderr, code) = run_cli(&f, &["foo", "--json"]);
    assert_eq!(code, Some(0));
    let value: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid JSON: {e}\n{stdout}"));
    let packages = value["packages"].as_array().expect("packages envelope");
    assert_eq!(packages.len(), 3);
    assert!(packages
        .iter()
        .all(|p| p.get("installed").is_some() && p.get("repositories").is_some()));

    // JSON modes parse as JSON.
    for args in [
        vec!["--repos", "--json"],
        vec!["--installed", "--json"],
        vec!["--installed", "--repo", SECURITY, "--json"],
        vec!["foo", "shared", "--json"],
    ] {
        let (stdout, _stderr, code) = run_cli(&f, &args);
        assert_eq!(code, Some(0), "args: {args:?}");
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap_or_else(|e| panic!("invalid JSON for {args:?}: {e}\n{stdout}"));
    }
}

#[test]
fn cli_human_tables_have_uniform_columns() {
    let f = Fixture::build("cli-uniform");

    // Every human package-listing mode emits the same four columns; a
    // --repo filter narrows the rows, never the columns.
    for args in [
        vec!["-i"],
        vec!["-i", "-r", SECURITY],
        vec!["-r", SECURITY],
        vec!["-a"],
        vec!["foo"],
        vec!["foo", "-r", DEBIAN],
        vec!["-m"],
    ] {
        let (stdout, _stderr, code) = run_cli(&f, &args);
        assert_eq!(code, Some(0), "args: {args:?}");
        let header = stdout.lines().next().unwrap_or_default();
        for col in ["PACKAGE", "VERSION", "ARCH", "REPOSITORY"] {
            assert!(header.contains(col), "args: {args:?} header: {header}");
        }
    }
}

#[test]
fn cli_repos_human_output_is_compact() {
    let f = Fixture::build("cli-repos");

    let (stdout, _stderr, code) = run_cli(&f, &["--repos"]);
    assert_eq!(code, Some(0));

    // One row per repository and suite (here: 4 repositories x 1 suite);
    // the wide SITE/ORIGIN/LABEL columns are only in the JSON output.
    assert_eq!(
        stdout.lines().count(),
        5,
        "header + one row per repository suite:\n{stdout}"
    );
    let header = stdout.lines().next().unwrap();
    for col in ["REPOSITORY", "SUITE", "COMPONENTS", "ARCHS"] {
        assert!(header.contains(col), "header: {header}");
    }
    assert!(!header.contains("SITE"), "header: {header}");
    assert!(!header.contains("ORIGIN"), "header: {header}");
    assert!(!header.contains("LABEL"), "header: {header}");

    // The REPOSITORY column keeps the full URI, copy-pasteable for --repo.
    let first_cells: Vec<String> = stdout
        .lines()
        .skip(1)
        .map(|l| l.split("  ").next().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        first_cells,
        vec![
            format!("{SECURITY}/"),
            format!("{UPDATES}/"),
            format!("{DEBIAN}/"),
            format!("{MIRROR}/"),
        ]
    );
    for (suite, archs) in [
        ("trixie-security", "amd64"),
        ("trixie-updates", "amd64"),
        ("trixie", "amd64, i386"),
    ] {
        assert!(
            stdout.contains(suite) && stdout.contains(archs),
            "suite {suite} / archs {archs} missing:\n{stdout}"
        );
    }
}

#[test]
fn cli_broken_pipe_exits_quietly() {
    // Enough packages that the table exceeds the 64 KiB pipe buffer: the
    // child cannot finish its write once the reader goes away, so the
    // EPIPE handling is deterministic.
    let f = Fixture::build_with_synthetic_packages("cli-pipe", 5000);

    let mut child = Command::new(env!("CARGO_BIN_EXE_apt-lists"))
        .env("APT_CONFIG", f.apt_config())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn apt-lists");

    // Close the read end (like `head` exiting): the child must fail its
    // write with EPIPE and exit quietly, not panic.
    drop(child.stdout.take());

    let out = child.wait_with_output().expect("wait for apt-lists");
    assert_eq!(out.status.code(), Some(141), "status: {out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
}

#[test]
fn cli_manual_installed() {
    let f = Fixture::build("cli-manual");

    // --manual-installed / -m implies the installed view, without the
    // auto-marked packages.
    for flag in ["--manual-installed", "-m"] {
        let (stdout, _stderr, code) = run_cli(&f, &[flag]);
        assert_eq!(code, Some(0), "flag: {flag}");
        assert!(stdout.contains("foo"), "{stdout}");
        assert!(stdout.contains("localonly"), "{stdout}");
        assert!(!stdout.contains("shared"), "auto package shown: {stdout}");
        assert!(!stdout.contains("debonly"), "auto package shown: {stdout}");
    }

    // Composes with --repo.
    let (stdout, _stderr, code) = run_cli(&f, &["-m", "-r", DEBIAN]);
    assert_eq!(code, Some(0));
    assert!(stdout.contains("foo"), "{stdout}");
    assert!(stdout.contains("secman"), "{stdout}");
    assert!(!stdout.contains("debonly"), "{stdout}");
}

#[test]
fn cli_shell_completion() {
    let f = Fixture::build("cli-completion");

    for shell in ["bash", "zsh", "fish", "elvish", "powershell"] {
        let (stdout, _stderr, code) = run_cli(&f, &["--generate-completion", shell]);
        assert_eq!(code, Some(0), "shell: {shell}");
        assert!(
            stdout.contains("apt-lists"),
            "completion for {shell} should reference the binary"
        );
    }

    // Unknown shells are rejected by clap.
    let (_stdout, _stderr, code) = run_cli(&f, &["--generate-completion", "tcsh"]);
    assert_eq!(code, Some(2));
}

#[test]
fn cli_errors_are_explicit_not_empty() {
    let f = Fixture::build("cli-errors");

    // Unknown repository: error, exit code 1, helpful list of known repos.
    let (_stdout, stderr, code) = run_cli(
        &f,
        &["--installed", "--repo", "https://nonexistent.invalid/apt"],
    );
    assert_eq!(code, Some(1));
    assert!(stderr.contains("was not found"), "stderr: {stderr}");
    assert!(
        stderr.contains(DEBIAN),
        "known repos are suggested: {stderr}"
    );

    // Ambiguous hostname: error, exit code 1.
    let (_stdout, stderr, code) = run_cli(&f, &["--installed", "--repo", "deb.debian.org"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("ambiguous"), "stderr: {stderr}");
    assert!(
        stderr.contains(SECURITY) && stderr.contains(DEBIAN),
        "stderr: {stderr}"
    );

    // Unknown package: error, exit code 1.
    let (_stdout, stderr, code) = run_cli(&f, &["no-such-pkg"]);
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no-such-pkg"), "stderr: {stderr}");

    // Existing repository without installed matches: exit 0, empty stdout.
    let (stdout, _stderr, code) = run_cli(&f, &["--installed", "--repo", UPDATES]);
    assert_eq!(code, Some(0));
    assert_eq!(stdout, "", "empty result must be empty, got:\n{stdout}");
}
