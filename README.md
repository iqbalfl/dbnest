# DBnest

Local database servers for Linux — pick an engine, pick a version, set a port,
hit Start. Runs natively: no Docker, no VM, no root.

DBnest is a DBngin-style tool for Linux. It uses none of the DBngin/TablePlus
names or assets.

> **Status: v0.1.0.** The bundled manifest carries real artifacts for all four
> engines on x86_64, so `dbnest start` genuinely installs and runs a server, and
> [v0.1.0](https://github.com/iqbalfl/dbnest/releases/tag/v0.1.0) ships built
> bundles. Not there yet: no aarch64, and cross-distro testing so far covers the
> Debian family only. See [What's not done](#whats-not-done).

## What exists

| Area | Status |
|---|---|
| PostgreSQL, Redis, MySQL, MariaDB | Adapters complete (DESIGN.md §7.2) |
| MongoDB | Not yet — out of scope for now |
| `dbnest` CLI | Complete per DESIGN.md §15 |
| GUI (Tauri v2 + React) | Server list, New Server, Connection/Logs/Settings, tray |
| Process backend | `systemd --user`, with a direct fallback |
| Preflight | root, libraries (`ldd`), port, socket path length, disk space |
| Autostart | Via an enabled systemd unit, or when the app opens |

## Installation

Grab an artifact from
[Releases](https://github.com/iqbalfl/dbnest/releases/tag/v0.1.0) —
`release.yml` produces an AppImage, a `.deb`, an `.rpm`, and a separate CLI
tarball, all x86_64:

```bash
# GUI + CLI
sudo dpkg -i DBnest_0.1.0_amd64.deb

# CLI only
tar xzf dbnest-cli-0.1.0-x86_64-linux.tar.gz
install -Dm755 dbnest ~/.local/bin/dbnest
```

`SHA256SUMS` in the same release covers every asset; check it before installing.
Or run from source (see [Development](#development)).

### Supported distros

The released bundles need **glibc ≥ 2.34**, because they are built on Ubuntu
22.04: Ubuntu 22.04+, Debian 12+, Fedora 35+, RHEL/Rocky 9+, Arch. Older
distros (Ubuntu 20.04, Debian 11, RHEL 8) have to build from source for now
— the code itself has no such floor. musl-based distros (Alpine) are not
supported.

The *engine* binaries DBnest downloads are a separate matter: those are built
on AlmaLinux 8 and gated at glibc 2.28, so they are not what limits the floor
(see [Building engine binaries](#building-engine-binaries)).

In practice CI exercises Ubuntu 24.04, Ubuntu 22.04 and Debian 12 — see
[Testing on the Debian family](#testing-on-the-debian-family). Debian 11 is not
provable in CI either way: bullseye left LTS in August 2026, and the `debian:11`
image now ships security-updated packages that no remaining repository serves,
so the container cannot be provisioned.

## CLI usage

```bash
dbnest engines                      # engines and versions in the manifest
dbnest versions [--installed]       # manifest versions / installed ones
dbnest create postgres 16.4 --name "Project A" --port 5433
dbnest start "Project A"            # installs and initialises if needed
dbnest list                         # instances with status and port
dbnest info "Project A"             # host/port/user/connection URL
dbnest logs "Project A" -n 200
dbnest shell "Project A"            # $SHELL with this engine's PATH and env
eval "$(dbnest env 'Project A')"    # the same env, in your current shell
dbnest stop "Project A"
dbnest delete "Project A" [--keep-data]
dbnest uninstall postgres 16.4      # remove a version (refused if still in use)
dbnest doctor                       # preflight every instance + system info
```

Every command accepts `--json` for scripting. Exit codes: `0` success,
`1` general error, `2` bad arguments, `3` preflight failed.

Instances can be referenced by id (`pg-7f3a2c`) or by name.

### Default credentials

As with DBngin, these servers are built for development: PostgreSQL uses the
`postgres` user with no password (`trust`), MySQL/MariaDB use `root` with no
password, and Redis has no auth. Every server listens on `127.0.0.1` only, with
no option to change that. **Do not use this in production.**

## Engine manifest

DBnest does not bundle engine binaries. Versions and their download URLs come
from a JSON manifest (DESIGN.md §5), resolved in this order: `manifest_url` from
settings → the last downloaded copy
(`$XDG_CACHE_HOME/dbnest/manifest.json`) → the copy embedded in the binary.

The bundled manifest holds real artifacts for all four engines on x86_64, every
one of them filled in by [`build-engines.yml`](#building-engine-binaries) rather
than typed by hand:

| Engine | Version | Artifact source | sha256 |
|---|---|---|---|
| Redis | 7.4.0 | this repo's Releases (built here) | measured at build time |
| PostgreSQL | 16.4 | this repo's Releases (built here) | measured at build time |
| MariaDB | 11.4.4 | archive.mariadb.org | matches upstream `sha256sums.txt` |
| MySQL | 8.4.3 | cdn.mysql.com (archive) | measured from the HTTPS download — see note |

**A note on MySQL:** upstream publishes no automatically fetchable checksum file
for that tarball, so the sha256 in the manifest was measured from the runner's
HTTPS download and has never been cross-checked against an official value.
PostgreSQL's and MariaDB's sources *were* cross-checked against upstream
checksums. That difference is deliberate and recorded rather than glossed over.

To add a version or an architecture, run `build-engines.yml` again. To ship new
versions without releasing a new app build, host the manifest yourself (GitHub
Pages or Releases) and set **Manifest URL** in Settings — the app fetches it on
launch or via **Refresh versions**.

Every artifact's sha256 is verified before extraction, and extraction rejects
archive entries with absolute paths or `..` in them — hard link targets
included.

## Building engine binaries

`.github/workflows/build-engines.yml` (run it manually via **Run workflow**)
fills the manifest with real data for all four engines, following DESIGN.md §19.

| Engine | How | URL in the manifest |
|---|---|---|
| Redis, PostgreSQL | built from source in an AlmaLinux 8 container | this repo's Releases |
| MySQL, MariaDB | official upstream tarball downloaded and verified | upstream's own URL |

Both paths converge on the same step: sha256 is measured on the runner, then a
pull request updates `manifest/manifest.json`. No value is ever typed by hand.
For MySQL and MariaDB the tarball's shape is checked too
(`scripts/check-tarball-layout.sh`), so that `strip_components: 1` really does
yield `bin/` at the root.

AlmaLinux 8 is used because its glibc is 2.28 — the oldest among supported
distros. `scripts/check-portable.sh` enforces that promise: the build fails if
any binary demands a newer glibc, or links a library whose SONAME differs across
distros. That is why PostgreSQL is built without ICU, readline, and OpenSSL (all
three have different SONAMEs on EL8 versus Ubuntu 24.04); the cost is that
`psql` has no line editing.

Each build is smoke-tested before packaging: Redis is started and `PING`ed, and
PostgreSQL is `initdb`'d and queried as an unprivileged user from a directory
different from its build prefix — which also proves the tree is relocatable.

PostgreSQL's source is verified against the official `.sha256` from
postgresql.org, and `scripts/verify-upstream-checksum.sh` does the same for
MySQL and MariaDB when upstream publishes one. When no checksum can be fetched
— as with Redis — the sha256 is measured from the runner's HTTPS download and
flagged with a warning in the log, never invented. Check it once against the
official download page, then pass it as the `redis_src_sha256` input so later
builds reject a changed source.

Prerequisite: **Settings → Actions → General → "Allow GitHub Actions to create
and approve pull requests"** must be enabled for the manifest PR step to work.

Only x86_64 is built so far; aarch64 is waiting on an ARM runner.

## Testing on the Debian family

The `integration` job in `ci.yml` (also manual) installs real engines from the
manifest and runs them on Ubuntu 24.04, Ubuntu 22.04, and Debian 12 — all four
engines per leg, end to end: download, checksum, extract, initialise, start,
query, stop, delete. This is what demonstrates that binaries from
`build-engines.yml` actually run on the target distros, rather than merely
compiling.

Because dbnest's preflight refuses to run as root (DESIGN §8), the container
legs compile the test binary as root and then execute it as an unprivileged
user. The tests also pin the process backend to `direct`: they isolate every
XDG path under a temporary directory, and a systemd unit written there is
somewhere systemd never reads. Covering the systemd backend needs a separate,
non-isolating test that does not exist yet.

Debian 11 was dropped from the matrix — see
[Supported distros](#supported-distros) for why.

## Data locations

Everything is per-user, XDG-compliant, and root-free:

```
$XDG_DATA_HOME/dbnest/      binaries/, instances/, compat-lib/, tmp/
$XDG_CONFIG_HOME/dbnest/    instances.json, settings.json
$XDG_CACHE_HOME/dbnest/     manifest.json, downloads/
~/.config/systemd/user/     dbnest-<id>.service (with the systemd backend)
```

## Process backend

`auto` (the default) uses `systemd --user` when available, and otherwise starts
the process directly with `setsid` so servers outlive the app. You can force
either in Settings; `dbnest doctor` reports which backend is active.

Under systemd, instances marked `autostart` start at login via an enabled unit.
To keep servers running without logging in at all:

```bash
loginctl enable-linger $USER
```

## Development

```bash
cargo build --workspace
cargo test --workspace
cargo fmt --all && cargo clippy --workspace -- -D warnings

cd ui && npm install && npm run dev   # frontend only
cargo tauri dev                        # full GUI, from the repo root

cargo run -p dbnest-cli -- list        # CLI
```

The integration tests download real binaries and run real servers, so they are
marked `#[ignore]`:

```bash
DBNEST_IT=1 cargo test -p dbnest-core -- --ignored
```

Now that the manifest carries real artifacts these can actually run, and CI's
`integration` job runs them across the Debian family.

### System dependencies for building the GUI

Ubuntu 22.04 and 24.04:

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev libsoup-3.0-dev patchelf
```

Tauri v2 needs webkit2gtk **4.1** (the `-4.0-dev` packages are for Tauri v1).
Add `xdg-utils` if you want to bundle an AppImage.

### Layout

```
crates/core/   dbnest-core — all business logic
crates/cli/    the `dbnest` binary
src-tauri/     Tauri v2 app (thin commands/events/tray over core)
ui/            React + TypeScript + Vite
manifest/      manifest.json, embedded as the fallback copy
```

The full design lives in [DESIGN.md](DESIGN.md).

## What's not done

- x86_64 only. aarch64 waits on an ARM runner, and that architecture's entries
  are removed from the manifest rather than left as `TODO`, so aarch64 users get
  an honest "version unavailable" instead of "not verified".
- MySQL 8.4.3's sha256 has not been cross-checked against an official upstream
  checksum (see [Engine manifest](#engine-manifest)); the others have.
- MongoDB is not supported.
- The systemd process backend has no integration test; only `direct` is
  exercised. Relatedly, changing the backend in Settings does not rebind a
  running app — `Manager` picks its backend once at construction, so the change
  only takes effect after a restart, with nothing telling the user that.
- The released bundles require glibc 2.34, so they exclude Ubuntu 20.04,
  Debian 11 and RHEL 8 even though the source builds there. Fixing that means
  building the release in an AlmaLinux 8 container the way `build-engines.yml`
  already does, which is not done yet.
- The `.deb`/`.rpm` package name comes out as `d-bnest` — Tauri derives it from
  `productName` ("DBnest") and offers no override.
- The project licence is undecided (`Cargo.toml` says MIT, but there is no
  `LICENSE` file).
- Manifest signing (minisign) does not exist yet, and the release assets are
  unsigned too — `SHA256SUMS` is the only integrity check, and it is served from
  the same place as the artifacts it covers.
- The app icons are still plain placeholders.
