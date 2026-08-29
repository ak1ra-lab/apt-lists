# Example output

The following transcript was generated with a synthetic fixture resembling a
Debian 13 system with official repositories configured:

```text
https://deb.debian.org/debian            trixie           (also mirrored at ftp.us.debian.org/debian)
https://deb.debian.org/debian-updates    trixie-updates
https://deb.debian.org/debian-security   trixie-security
```

Installed state:

```text
foo      2.0-1          amd64/i386  (archive + mirror provide 2.0-1; updates has 2.0-1+deb13u1)
libbaz   3.1-2+deb13u1  amd64       (security update from debian-security)
seconly  2.0            all         (security pocket only)
shared   5.0            all         (identical version in archive and mirror)
debonly  1.0            all         (archive only)
local    localonly 4.2  all         (installed from a local .deb, in no repository)
```

Auto-install marks (`/var/lib/apt/extended_states`): `shared` and `debonly`
are automatic; everything else counts as manually installed.

```console
$ apt-lists --installed --repo https://deb.debian.org/debian-security/
PACKAGE  VERSION        ARCH   REPOSITORY
libbaz   3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/
seconly  2.0            all    https://deb.debian.org/debian-security/

$ apt-lists --installed --repo deb.debian.org   # ambiguous hostname
apt-lists: error: repository selector 'deb.debian.org' is ambiguous; it matches multiple repository URIs:
  https://deb.debian.org/debian-security/
  https://deb.debian.org/debian-updates/
  https://deb.debian.org/debian/
pass the full repository URI to disambiguate
exit code: 1

$ apt-lists --installed --repo deb.debian.org/debian-security   # path disambiguates
PACKAGE  VERSION        ARCH   REPOSITORY
libbaz   3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/
seconly  2.0            all    https://deb.debian.org/debian-security/

$ apt-lists --installed   # (-i also works)
PACKAGE    VERSION        ARCH   REPOSITORY
debonly    1.0            all    https://deb.debian.org/debian/
foo        2.0-1          amd64  https://deb.debian.org/debian/, https://ftp.us.debian.org/debian/
foo        2.0-1          i386   https://deb.debian.org/debian/
libbaz     3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/
localonly  4.2            all    -
secman     1.0            all    https://deb.debian.org/debian/
seconly    2.0            all    https://deb.debian.org/debian-security/
shared     5.0            all    https://deb.debian.org/debian/

$ apt-lists --repos   # (-R also works)
REPOSITORY                               SUITE            COMPONENTS  ARCHS
https://deb.debian.org/debian-security/  trixie-security  main        amd64
https://deb.debian.org/debian-updates/   trixie-updates   main        amd64
https://deb.debian.org/debian/           trixie           main        amd64, i386
https://ftp.us.debian.org/debian/        trixie           main        amd64

$ apt-lists foo
PACKAGE  VERSION        ARCH   REPOSITORY
foo      2.0-1+deb13u1  amd64  https://deb.debian.org/debian-updates/
foo      2.0-1          amd64  https://deb.debian.org/debian/, https://ftp.us.debian.org/debian/  [installed]
foo      2.0-1          i386   https://deb.debian.org/debian/  [installed]

$ apt-lists --manual-installed   # (-m also works)
PACKAGE    VERSION        ARCH   REPOSITORY
foo        2.0-1          amd64  https://deb.debian.org/debian/, https://ftp.us.debian.org/debian/
foo        2.0-1          i386   https://deb.debian.org/debian/
libbaz     3.1-2+deb13u1  amd64  https://deb.debian.org/debian-security/
localonly  4.2            all    -
secman     1.0            all    https://deb.debian.org/debian/
seconly    2.0            all    https://deb.debian.org/debian-security/

$ apt-lists --installed --repo https://deb.debian.org/debian-security/ --json
{
  "packages": [
    {
      "architecture": "amd64",
      "installed": true,
      "name": "libbaz",
      "version": "3.1-2+deb13u1"
    },
    {
      "architecture": "all",
      "installed": true,
      "name": "seconly",
      "version": "2.0"
    }
  ],
  "repository": {
    "label": "Debian Security",
    "origin": "Debian Security",
    "site": "deb.debian.org",
    "suites": [
      {
        "architectures": [
          "amd64"
        ],
        "archive": "trixie-security",
        "codename": "trixie",
        "components": [
          "main"
        ]
      }
    ],
    "uri": "https://deb.debian.org/debian-security/"
  }
}

$ apt-lists --installed --repo https://nonexistent.invalid/apt; echo "exit code: $?"
apt-lists: error: repository 'https://nonexistent.invalid/apt' was not found in the APT cache
repositories known to the cache:
  https://deb.debian.org/debian-security/
  https://deb.debian.org/debian-updates/
  https://deb.debian.org/debian/
  https://ftp.us.debian.org/debian/

exit code: 1

$ apt-lists --installed | head -n 1   # pipelines work; no broken-pipe panic
PACKAGE    VERSION        ARCH   REPOSITORY
```

