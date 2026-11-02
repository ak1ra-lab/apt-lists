# Releasing apt-lists

Releases are tag-driven. The `release` workflow
(`.github/workflows/release.yaml`) runs the reusable `ci` workflow against the
tag, then publishes the source package to crates.io via Trusted Publishing
(GitHub OIDC). It does not build binary artifacts or create a GitHub Release.

## One-time setup

crates.io only allows a trusted publisher to be configured after the crate
exists, so the first version is published manually:

1. Create a crates.io API token with `publish-new` and `publish-update` scope,
   then run `cargo login`.
2. Publish the first version from a clean checkout:

   ```console
   cargo publish --locked
   ```

3. In crates.io, open **apt-lists → Settings → Trusted Publishing** and add:

   | field       | value          |
   | ----------- | -------------- |
   | owner       | `ak1ra-lab`    |
   | repository  | `apt-lists`    |
   | workflow    | `release.yaml` |
   | environment | `release`      |

4. In GitHub, create the `release` environment under **Settings →
   Environments** (optionally require a reviewer).
5. Revoke the token created in step 1.

## Regular release

1. Bump `version` in `Cargo.toml` following Semantic Versioning.
2. Refresh the lockfile entry for the local crate and run the local gate:

   ```console
   cargo check
   just check
   cargo publish --dry-run --locked
   ```

3. Commit and push to `master`; wait for the `ci` workflow to pass.
4. Create and push the annotated tag:

   ```console
   git tag -a v0.1.0 -m "apt-lists v0.1.0"
   git push origin v0.1.0
   ```

5. Watch the `release` workflow. It runs CI against the tag, checks crates.io,
   and publishes with a short-lived OIDC token.

If the version is already on crates.io, the workflow skips publishing. This is
why the first manually published version can still be tagged afterwards.

`cargo publish --no-verify` is used because `ci.yaml` already ran the tests and
`cargo package` on the same tag.
