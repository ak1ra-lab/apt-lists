# Local development tasks for apt-lists.
#
# Requires `just` (https://github.com/casey/just) and a Debian system with
# libapt-pkg-dev installed.

# Fail on any rustfmt/clippy difference; CI runs the same commands.
flag_strict := if env_var_or_default("CI", "") != "" { "--" } else { "" }

# show available recipes
default:
    @just --list --unsorted

# fmt + clippy + test: everything CI enforces.
check: fmt-check lint test

# Format all code.
fmt:
    cargo fmt

# Verify formatting only.
fmt-check:
    cargo fmt --check

# Lint (lib, bins, tests) with the configured [lints] from Cargo.toml.
lint:
    cargo clippy --all-targets -- -D warnings

# Run the full test suite (never touches the host APT state).
test:
    cargo test

# Optimized build.
build:
    cargo build --release

# Print shell completion for a shell: just completion bash|zsh|fish|elvish|powershell
completion shell:
    cargo run --quiet -- --generate-completion {{shell}}

# Check dependencies for known advisories (requires cargo-audit).
audit:
    cargo audit
