//! Human-readable output, kept in the spirit of `apt list`.
//!
//! Every package-listing mode renders the same four columns
//! (PACKAGE/VERSION/ARCH/REPOSITORY); a `--repo` filter narrows the rows,
//! never the columns. The repository catalog table (`--repos`) is one row
//! per repository and suite; the wider Release-file metadata (origin, label,
//! codename, index filenames) is available through `--json`.

/// Render a table with a header row; columns are as wide as their widest
/// cell. Used instead of a table crate to keep output stable and compact.
#[must_use]
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            rows.iter()
                .map(|r| r.get(i).map_or(0, |c| c.chars().count()))
                .max()
                .unwrap_or(0)
                .max(h.chars().count())
        })
        .collect();

    let mut out = String::new();
    out.push_str(&row_line(headers, &widths));
    for r in rows {
        out.push_str(&row_line(r, &widths));
    }
    out
}

fn row_line<S: AsRef<str>>(cells: &[S], widths: &[usize]) -> String {
    let mut line = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let cell = cell.as_ref();
        let width = widths[i];
        line.push_str(cell);
        let pad = width.saturating_sub(cell.chars().count());
        // Two spaces of separation; no trailing padding on the last column.
        if i + 1 < cells.len() {
            line.push_str(&" ".repeat(pad + 2));
        }
    }
    line.push('\n');
    line
}

/// Join repository URIs for the human REPOSITORY column. The full URI is
/// kept verbatim so the value can be passed to `--repo` unchanged.
#[must_use]
pub fn repo_column(repos: &[crate::repository::RepoIndex]) -> String {
    if repos.is_empty() {
        return "-".to_string();
    }

    let mut uris: Vec<String> = Vec::new();
    for idx in repos {
        if !uris.contains(&idx.uri) {
            uris.push(idx.uri.clone());
        }
    }
    uris.join(", ")
}

/// Format installed-version rows (`--installed`, optionally `--repo`).
///
/// The REPOSITORY column is always present: without a filter it lists every
/// repository that provides the exact installed version (`-` when none
/// does), with a filter it repeats the selected repository.
#[must_use]
pub fn installed(rows: &[crate::query::VersionRow]) -> String {
    let headers = ["PACKAGE", "VERSION", "ARCH", "REPOSITORY"];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            vec![
                r.name.clone(),
                r.version.clone(),
                r.arch.clone(),
                repo_column(&r.repositories),
            ]
        })
        .collect();
    table(&headers, &body)
}

/// Format the repository catalog (`--repos`): one row per repository and
/// suite, with the components and index architectures served for that suite
/// and the number of distinct package names the suite provides (an
/// `Architecture: all` package listed in several architecture indexes of one
/// suite counts once). SITE is omitted because it is the host part of the
/// REPOSITORY URI, and ORIGIN/LABEL are Release-file metadata not related to
/// the host; both stay available through `--json`.
#[must_use]
pub fn repos(
    catalog: &crate::repository::RepoCatalog,
    counts: &crate::apt::PackageCounts,
) -> String {
    let headers = ["REPOSITORY", "SUITE", "COMPONENTS", "ARCHS", "PACKAGES"];
    let mut body: Vec<Vec<String>> = Vec::new();

    for repo in catalog.repositories() {
        let suites = repo.archives();
        if suites.is_empty() {
            // No suite metadata at all (e.g. a flat repository).
            body.push(vec![
                repo.uri.clone(),
                "-".to_string(),
                join_or_dash(&distinct_over(repo.indexes.iter(), |i| i.component.clone())),
                join_or_dash(&distinct_over(repo.indexes.iter(), |i| i.arch.clone())),
                crate::apt::suite_package_count(counts, &repo.uri, None).to_string(),
            ]);
            continue;
        }
        for suite in suites {
            let in_suite =
                |i: &crate::repository::RepoIndex| i.archive.as_deref() == Some(suite.as_str());
            let components = distinct_over(repo.indexes.iter().filter(|i| in_suite(i)), |i| {
                i.component.clone()
            });
            let archs = distinct_over(repo.indexes.iter().filter(|i| in_suite(i)), |i| {
                i.arch.clone()
            });
            let packages = crate::apt::suite_package_count(counts, &repo.uri, Some(&suite));
            body.push(vec![
                repo.uri.clone(),
                suite,
                join_or_dash(&components),
                join_or_dash(&archs),
                packages.to_string(),
            ]);
        }
    }

    table(&headers, &body)
}

/// Format version rows for one or more packages (`apt-lists <package>`).
#[must_use]
pub fn package_versions(rows: &[crate::query::VersionRow]) -> String {
    let headers = ["PACKAGE", "VERSION", "ARCH", "REPOSITORY"];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut repo = repo_column(&r.repositories);
            if r.installed {
                repo.push_str("  [installed]");
            }
            vec![r.name.clone(), r.version.clone(), r.arch.clone(), repo]
        })
        .collect();
    table(&headers, &body)
}

/// Distinct non-empty values of `get` over the given indexes, in order.
fn distinct_over<'a, I>(
    indexes: I,
    get: impl Fn(&crate::repository::RepoIndex) -> Option<String>,
) -> Vec<String>
where
    I: IntoIterator<Item = &'a crate::repository::RepoIndex>,
{
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

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(", ")
    }
}
