//! Human-readable output, kept in the spirit of `apt list`.

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

/// Join repository URIs for the compact human REPOSITORY column, using the
/// given display function (raw URI or scheme-stripped).
#[must_use]
pub fn repo_column(repos: &[crate::repository::RepoIndex], shorten: bool) -> String {
    if repos.is_empty() {
        return "-".to_string();
    }

    let mut uris: Vec<String> = Vec::new();
    for idx in repos {
        let uri = if shorten {
            crate::repository::strip_scheme(&idx.uri)
        } else {
            idx.uri.clone()
        };
        if !uris.contains(&uri) {
            uris.push(uri);
        }
    }
    uris.join(", ")
}

/// Format installed-version rows (`--installed`, optionally `--repo`).
///
/// With a repository filter only PACKAGE/VERSION/ARCH are shown (the
/// repository is implied by the filter); without a filter an extra
/// REPOSITORY column lists every repository that provides the exact
/// installed version.
#[must_use]
pub fn installed(rows: &[crate::query::VersionRow], repo_filtered: bool) -> String {
    if repo_filtered {
        let headers = ["PACKAGE", "VERSION", "ARCH"];
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|r| vec![r.name.clone(), r.version.clone(), r.arch.clone()])
            .collect();
        table(&headers, &body)
    } else {
        let headers = ["PACKAGE", "VERSION", "ARCH", "REPOSITORY"];
        let body: Vec<Vec<String>> = rows
            .iter()
            .map(|r| {
                vec![
                    r.name.clone(),
                    r.version.clone(),
                    r.arch.clone(),
                    repo_column(&r.repositories, false),
                ]
            })
            .collect();
        table(&headers, &body)
    }
}

/// Format the repository catalog (`--repos`): one row per repository URI with
/// its most useful release metadata. SITE is the hostname; ORIGIN/LABEL are
/// distinct Release-file fields and are not related to the hostname.
#[must_use]
pub fn repos(catalog: &crate::repository::RepoCatalog) -> String {
    let headers = [
        "REPOSITORY",
        "SITE",
        "SUITE",
        "COMPONENTS",
        "ARCHS",
        "ORIGIN",
        "LABEL",
    ];
    let body: Vec<Vec<String>> = catalog
        .repositories()
        .iter()
        .map(|r| {
            vec![
                r.uri.clone(),
                r.site.clone().unwrap_or_else(|| "-".to_string()),
                join_or_dash(&r.archives()),
                join_or_dash(&r.components()),
                join_or_dash(&r.archs()),
                join_or_dash(&r.origins()),
                join_or_dash(&r.labels()),
            ]
        })
        .collect();
    table(&headers, &body)
}

/// Format version rows for one or more packages (`apt-lists <package>`).
#[must_use]
pub fn package_versions(rows: &[crate::query::VersionRow]) -> String {
    let headers = ["PACKAGE", "VERSION", "ARCH", "REPOSITORY"];
    let body: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut repo = repo_column(&r.repositories, false);
            if r.installed {
                repo.push_str("  [installed]");
            }
            vec![r.name.clone(), r.version.clone(), r.arch.clone(), repo]
        })
        .collect();
    table(&headers, &body)
}

fn join_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_string()
    } else {
        values.join(", ")
    }
}
