//! Command line interface.
//!
//! The option surface deliberately mirrors `apt list` (flat options instead
//! of a command hierarchy): `--installed`, `--repo`, `--all`, `--repos`,
//! `--json`, plus short forms and shell completion generation.

use std::io::{BufWriter, Write};

use clap::{CommandFactory, Parser};
use clap_complete::{Generator, Shell};

use crate::error::AptListsError;

/// A read-only, repository-aware companion to `apt list`.
///
/// Answers: "which installed package versions are currently provided by a
/// specific APT repository?" — according to the current local APT cache.
#[derive(Debug, Parser)]
#[command(
    name = "apt-lists",
    version,
    after_help = "Provenance note: matches are based on the current local APT \
                 cache and mean \"this exact installed version is currently \
                 available from this repository\"; they do not prove where a \
                 package was originally installed from."
)]
// The mode flags are naturally independent booleans; a state machine would
// not make the clap surface clearer.
#[allow(clippy::struct_excessive_bools)]
pub struct Args {
    /// List all available package versions (default when no mode is given).
    #[arg(short = 'a', long, conflicts_with = "packages")]
    pub all: bool,

    /// Show packages with an installed version (dpkg state), like
    /// `apt list --installed`.
    #[arg(short = 'i', long, conflicts_with_all = ["repos", "all", "packages"])]
    pub installed: bool,

    /// Show only manually installed packages (implies `--installed`), like
    /// `apt list --manual-installed`.
    #[arg(short = 'm', long, conflicts_with_all = ["repos", "all", "packages"])]
    pub manual_installed: bool,

    /// Only consider this repository. Accepts a repository URI
    /// (`https://deb.debian.org/debian-security/`) or a hostname
    /// (`security.debian.org`).
    #[arg(short = 'r', long, value_name = "REPOSITORY", conflicts_with = "repos")]
    pub repo: Option<String>,

    /// List all repositories known to the current APT cache.
    #[arg(short = 'R', long, conflicts_with_all = ["installed", "all", "packages"])]
    pub repos: bool,

    /// Emit machine-readable JSON instead of a table.
    #[arg(short = 'j', long)]
    pub json: bool,

    /// Print a shell completion script for the given shell to stdout and
    /// exit, e.g. `apt-lists --generate-completion bash`.
    #[arg(long, value_enum, hide_possible_values = true)]
    pub generate_completion: Option<Shell>,

    /// Package name(s) to inspect; accepts `name` or `name:arch`.
    #[arg(value_name = "PACKAGE")]
    pub packages: Vec<String>,
}

impl Args {
    /// Which top-level mode was requested.
    ///
    /// Precedence: `--repos` > `--installed`/`--manual-installed` > package
    /// query > `--all`. A bare `apt-lists` (and `--all`) lists all available
    /// package versions, mirroring `apt list`.
    #[must_use]
    pub fn mode(&self) -> Mode {
        if self.repos {
            return Mode::Repos;
        }
        if self.installed || self.manual_installed {
            return Mode::Installed;
        }
        if self.packages.is_empty() {
            return Mode::All;
        }
        Mode::Packages
    }

    /// `true` when only manually installed packages should be shown.
    #[must_use]
    pub fn manual_only(&self) -> bool {
        self.manual_installed
    }

    /// Print the completion script for `shell` (if requested).
    ///
    /// Returns `Ok(true)` when a completion script was printed and the
    /// program should exit successfully.
    ///
    /// # Errors
    ///
    /// Returns an error when the script cannot be written to stdout, e.g.
    /// because the reader closed the pipe (`... | head`).
    pub fn print_completion(&self) -> Result<bool, AptListsError> {
        let Some(shell) = self.generate_completion else {
            return Ok(false);
        };
        let mut cmd = Args::command();
        cmd.build();
        cmd.set_bin_name("apt-lists");
        // Buffered so a closed stdout surfaces here as a `BrokenPipe` error
        // on the flush below instead of a panic inside clap_complete.
        let mut out = BufWriter::new(std::io::stdout().lock());
        shell.generate(&cmd, &mut out);
        out.flush()?;
        Ok(true)
    }
}

/// Which top-level mode was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `--repos`: list the repository catalog.
    Repos,
    /// `--installed`/`--manual-installed`: list installed versions.
    Installed,
    /// `--all` (or no mode): list all available versions.
    All,
    /// Positional package names: list their versions.
    Packages,
}
