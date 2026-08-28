//! `apt-lists` — read-only, repository-aware companion to `apt list`.
//!
//! Built on top of [`oma_apt`] (libapt-pkg bindings). The library part is
//! split into:
//!
//! * [`apt`] — cache initialization and the single cache scan,
//! * [`repository`] — repository identity from `PackageFile` metadata,
//! * [`query`] — installed/all/package queries with repository filtering,
//! * [`output`] — human tables and JSON,
//! * [`cli`] — argument parsing.

pub mod apt;
pub mod cli;
pub mod error;
pub mod output;
pub mod query;
pub mod repository;
