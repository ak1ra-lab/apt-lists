//! `apt-lists` binary: wires the CLI to the library and prints results.
//!
//! See the library documentation (`apt_lists`) for the modules and the
//! README for usage, provenance semantics and examples.

use std::io::Write;

use clap::Parser;

use apt_lists::apt;
use apt_lists::cli::{Args, Mode};
use apt_lists::error::AptListsError;
use apt_lists::output::{human, json};
use apt_lists::query::{self, InstalledFilter, RepoFilter, VersionRow};
use apt_lists::repository::{RepoCatalog, ResolvedRepo};

fn main() {
    let args = Args::parse();

    if let Err(err) = run(&args) {
        // A closed stdout pipe (`apt-lists -i | head`) is normal usage, not
        // an error: exit quietly with the conventional `128 + SIGPIPE`
        // status instead of panicking the way the print macros would.
        if matches!(
            &err,
            AptListsError::Output(e) if e.kind() == std::io::ErrorKind::BrokenPipe
        ) {
            std::process::exit(141);
        }
        eprintln!("apt-lists: error: {err}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), AptListsError> {
    if args.print_completion()? {
        return Ok(());
    }

    let cache = apt::open_cache()?;
    let scan = apt::scan(&cache);
    let catalog = RepoCatalog::from_indexes(&scan.indexes);

    // Resolve --repo up front so an invalid selector is reported even before
    // looking at packages, and so "selector does not exist" is never
    // mistaken for "exists but nothing matched".
    let resolved = match &args.repo {
        Some(selector) => {
            if catalog.is_empty() {
                return Err(AptListsError::NoPackageIndexes);
            }
            Some(catalog.resolve(selector)?)
        }
        None => None,
    };

    match args.mode() {
        Mode::Repos => {
            if catalog.is_empty() {
                return Err(AptListsError::NoPackageIndexes);
            }
            if args.json {
                write_stdout(&json::to_pretty(&json::repos(
                    &catalog,
                    &scan.package_counts,
                )))?;
            } else {
                write_stdout(&human::repos(
                    &catalog,
                    &scan.package_counts,
                    args.no_headers,
                ))?;
            }
        }
        Mode::Installed => {
            let installed_filter = InstalledFilter {
                manual_only: args.manual_only(),
            };
            let rows = query::installed(
                &scan,
                &catalog,
                filter_of(resolved.as_ref()),
                installed_filter,
            );
            if catalog.is_empty() {
                no_indexes_warning();
            }
            emit_rows(args, resolved.as_ref(), &rows)?;
        }
        Mode::All => {
            if catalog.is_empty() {
                no_indexes_warning();
            }
            let rows = query::all_versions(&scan, &catalog, filter_of(resolved.as_ref()));
            emit_rows(args, resolved.as_ref(), &rows)?;
        }
        Mode::Packages => {
            if catalog.is_empty() {
                no_indexes_warning();
            }
            let filter = filter_of(resolved.as_ref());
            let mut rows = Vec::new();
            for name in &args.packages {
                rows.extend(query::package_versions(&scan, &catalog, name, filter)?);
            }
            emit_rows(args, resolved.as_ref(), &rows)?;
        }
    }

    Ok(())
}

fn filter_of(resolved: Option<&ResolvedRepo>) -> RepoFilter<'_> {
    match resolved {
        Some(sel) => RepoFilter::Selected(sel),
        None => RepoFilter::None,
    }
}

/// Emit package/version rows for every package-listing mode. The shapes are
/// uniform across modes:
///
/// * JSON without `--repo`: `{ "packages": [...] }`, each row carries its
///   repositories and the installed flag;
/// * JSON with `--repo`: `{ "repository": ..., "packages": [...] }`, the
///   selected repository is reported once at the top level;
/// * human: always the PACKAGE/VERSION/ARCH/REPOSITORY columns.
fn emit_rows(
    args: &Args,
    resolved: Option<&ResolvedRepo>,
    rows: &[VersionRow],
) -> Result<(), AptListsError> {
    if args.json {
        let value = match resolved {
            Some(sel) => json::for_repo(&sel.repository, rows),
            None => json::versions(rows),
        };
        write_stdout(&json::to_pretty(&value))
    } else if !rows.is_empty() {
        let text = match args.mode() {
            Mode::Installed => human::installed(rows, args.no_headers),
            _ => human::package_versions(rows, args.no_headers),
        };
        write_stdout(&text)
    } else {
        Ok(())
    }
}

/// Write to stdout, propagating OS errors (a broken pipe is handled by the
/// caller as a normal end of pipeline, not as an error report).
fn write_stdout(text: &str) -> Result<(), AptListsError> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(text.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

fn no_indexes_warning() {
    eprintln!(
        "apt-lists: warning: no APT package indexes are available; \
         repository provenance cannot be determined"
    );
}
