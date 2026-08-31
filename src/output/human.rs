//! Human-readable output, kept in the spirit of `apt list`.
//!
//! Every package-listing mode renders the same four columns
//! (PACKAGE/VERSION/ARCH/REPOSITORY); a `--repo` filter narrows the rows,
//! never the columns. The repository catalog table (`--repos`) is one row
//! per repository and suite; the wider Release-file metadata (origin, label,
//! codename, index filenames) is available through `--json`.
//!
//! Cells are kept pipeline-friendly: multi-value cells are joined by a
//! comma without a space, so every cell is a single whitespace-free token
//! and the alignment padding is the column separator. With `--no-headers`
//! the header row is dropped and rows can be piped straight into
//! `sort -k<n>` or `awk`. The one exception is the row-final `[installed]`
//! marker of package queries, which follows the REPOSITORY cell after two
//! spaces.

/// Render a table; columns are as wide as their widest cell. Used instead of
/// a table crate to keep output stable and compact.
///
/// The header row is omitted when `headers` is `None` (`--no-headers`); the
/// column widths are then computed from the body alone.
#[must_use]
pub fn table(headers: Option<&[&str]>, rows: &[Vec<String>]) -> String {
    let header_row: &[&str] = headers.unwrap_or_default();
    let ncols = if header_row.is_empty() {
        rows.first().map_or(0, Vec::len)
    } else {
        header_row.len()
    };
    let widths: Vec<usize> = (0..ncols)
        .map(|i| {
            let body = rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|c| c.chars().count());
            let header = header_row.get(i).map_or(0, |h| h.chars().count());
            body.chain(std::iter::once(header)).max().unwrap_or(0)
        })
        .collect();

    let mut out = String::new();
    if !header_row.is_empty() {
        out.push_str(&row_line(header_row, &widths));
    }
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
/// kept verbatim so the value can be passed to `--repo` unchanged; multiple
/// URIs are joined by a comma without a space to keep the cell a single
/// whitespace-free token.
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
    uris.join(",")
}

/// Format installed-version rows (`--installed`, optionally `--repo`).
///
/// The REPOSITORY column is always present: without a filter it lists every
/// repository that provides the exact installed version (`-` when none
/// does), with a filter it repeats the selected repository. The header row
/// is omitted with `no_headers`.
#[must_use]
pub fn installed(rows: &[crate::query::VersionRow], no_headers: bool) -> String {
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
    table(header_if(&headers, !no_headers), &body)
}

/// Format the repository catalog (`--repos`): one row per repository and
/// suite, with the components and index architectures served for that suite
/// and the number of distinct package names the suite provides (an
/// `Architecture: all` package listed in several architecture indexes of one
/// suite counts once). SITE is omitted because it is the host part of the
/// REPOSITORY URI, and ORIGIN/LABEL are Release-file metadata not related to
/// the host; both stay available through `--json`. The header row is omitted
/// with `no_headers`.
#[must_use]
pub fn repos(
    catalog: &crate::repository::RepoCatalog,
    counts: &crate::apt::PackageCounts,
    no_headers: bool,
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

    table(header_if(&headers, !no_headers), &body)
}

/// Format version rows for one or more packages (`apt-lists <package>`).
/// The `[installed]` marker is a row-final token after the REPOSITORY cell;
/// the header row is omitted with `no_headers`.
#[must_use]
pub fn package_versions(rows: &[crate::query::VersionRow], no_headers: bool) -> String {
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
    table(header_if(&headers, !no_headers), &body)
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

/// The header argument for [`table`]: the header row, or `None` to omit it.
fn header_if<'a>(headers: &'a [&'a str], show: bool) -> Option<&'a [&'a str]> {
    if show {
        Some(headers)
    } else {
        None
    }
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(",")
    }
}
