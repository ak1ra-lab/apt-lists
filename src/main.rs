//! `apt-lists` binary: wires the CLI to the library and prints results.
//!
//! See the library documentation (`apt_lists`) for the modules and the
//! README for usage, provenance semantics and examples.

use clap::Parser;

use apt_lists::apt;
use apt_lists::cli::{Args, Mode};
use apt_lists::error::AptListsError;
use apt_lists::output::{human, json};
use apt_lists::query::{self, InstalledFilter, RepoFilter};
use apt_lists::repository::RepoCatalog;

fn main() {
    let args = Args::parse();

    if args.print_completion() {
        return;
    }

    if let Err(err) = run(&args) {
        eprintln!("apt-lists: error: {err}");
        std::process::exit(1);
    }
}

fn run(args: &Args) -> Result<(), AptListsError> {
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
                println!("{}", json::to_pretty(&json::repos(&catalog)));
            } else {
                print!("{}", human::repos(&catalog));
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
            emit_rows(args, resolved.as_ref(), &rows);
        }
        Mode::All => {
            if catalog.is_empty() {
                no_indexes_warning();
            }
            let rows = with_filter(&scan, &catalog, resolved.as_ref(), query::all_versions);
            emit_rows(args, resolved.as_ref(), &rows);
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
            emit_package_rows(args, resolved.as_ref(), &rows);
        }
    }

    Ok(())
}

fn with_filter(
    scan: &apt::CacheScan,
    catalog: &RepoCatalog,
    resolved: Option<&apt_lists::repository::ResolvedRepo>,
    f: fn(&apt::CacheScan, &RepoCatalog, RepoFilter<'_>) -> Vec<apt_lists::query::VersionRow>,
) -> Vec<apt_lists::query::VersionRow> {
    f(scan, catalog, filter_of(resolved))
}

fn filter_of(resolved: Option<&apt_lists::repository::ResolvedRepo>) -> RepoFilter<'_> {
    match resolved {
        Some(sel) => RepoFilter::Selected(sel),
        None => RepoFilter::None,
    }
}

fn emit_rows(
    args: &Args,
    resolved: Option<&apt_lists::repository::ResolvedRepo>,
    rows: &[apt_lists::query::VersionRow],
) {
    if args.json {
        match resolved {
            Some(sel) => println!(
                "{}",
                json::to_pretty(&json::for_repo(&sel.repository, rows))
            ),
            None => println!("{}", json::to_pretty(&json::versions(rows))),
        }
    } else if !rows.is_empty() {
        let repo_filtered = args.repo.is_some();
        let text = match args.mode() {
            Mode::Installed => human::installed(rows, repo_filtered),
            _ => human::package_versions(rows),
        };
        print!("{text}");
    }
}

fn emit_package_rows(
    args: &Args,
    resolved: Option<&apt_lists::repository::ResolvedRepo>,
    rows: &[apt_lists::query::VersionRow],
) {
    if args.json {
        match resolved {
            Some(sel) => println!(
                "{}",
                json::to_pretty(&json::for_repo(&sel.repository, rows))
            ),
            None => {
                if args.packages.len() == 1 {
                    println!(
                        "{}",
                        json::to_pretty(&json::package(&args.packages[0], rows))
                    );
                } else {
                    println!("{}", json::to_pretty(&json::versions(rows)));
                }
            }
        }
    } else if !rows.is_empty() {
        print!("{}", human::package_versions(rows));
    }
}

fn no_indexes_warning() {
    eprintln!(
        "apt-lists: warning: no APT package indexes are available; \
         repository provenance cannot be determined"
    );
}
