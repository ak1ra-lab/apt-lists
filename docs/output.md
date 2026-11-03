# Output formats

## Human tables

Human package tables always use the same columns:
`PACKAGE`, `VERSION`, `ARCH`, `REPOSITORY`. A `--repo` filter narrows the
rows, never the columns. `--repos` prints one row per repository and suite,
with a `PACKAGES` column, so the table stays narrow.

### Piping tables

Table cells are single whitespace-free tokens: multi-value cells (components,
archs, repository URIs) are joined by a comma **without** a space, so the
alignment padding is the only whitespace and `sort -k<n>` / `awk` address
columns directly. `--no-headers` drops the header row (the column widths are
then computed from the data alone):

```console
$ apt-lists --repos --no-headers | sort -k5,5n
https://deb.debian.org/debian-security/  trixie-security  main  amd64       2
https://deb.debian.org/debian-updates/   trixie-updates   main  amd64       2
https://ftp.us.debian.org/debian/        trixie           main  amd64       3
https://deb.debian.org/debian/           trixie           main  amd64,i386  5
```

The one exception is the row-final `[installed]` marker of package queries,
which follows the REPOSITORY cell after two spaces (an extra token, so
`-k1..-k4` still address the columns).

## JSON

Every package-listing mode uses the same two shapes, so scripts can rely on
a uniform field set:

* without `--repo`: `{ "packages": [ ... ] }`, every row carries its
  `repositories` array and the `installed` flag;
* with `--repo`: `{ "repository": {...}, "packages": [ ... ] }`, the selected
  repository is reported once at the top level, so the rows omit their
  (redundant) `repositories` array.

```console
$ apt-lists --installed --repo https://deb.debian.org/debian-security/ --json
```

```json
{
  "repository": {
    "uri": "https://deb.debian.org/debian-security/",
    "site": "deb.debian.org",
    "origin": "Debian Security",
    "label": "Debian Security",
    "suites": [
      {
        "archive": "trixie-security",
        "codename": "trixie",
        "components": ["main"],
        "architectures": ["amd64"]
      }
    ]
  },
  "packages": [
    {
      "name": "libbaz",
      "version": "3.1-2+deb13u1",
      "architecture": "amd64",
      "installed": true
    },
    {
      "name": "seconly",
      "version": "2.0",
      "architecture": "all",
      "installed": true
    }
  ]
}
```

Single-package queries (`apt-lists foo --json`) use the same `packages`
envelope as every other mode. With `--repos --json`, the per-suite detail
(suites with codename, components, architectures and the number of distinct
package names as `packages`) and the Release metadata (`origin`, `label`,
per-index `index_filename`/`index_type`) are reported in full. No canonical
URI reconstruction is promised: the values are the raw ones libapt-pkg
exposes.

`serde_json` does not preserve insertion order, so JSON object keys are
printed alphabetically. The transcript in
[example-output.md](example-output.md) shows the exact bytes, including key
order.
