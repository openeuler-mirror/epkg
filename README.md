# epkg

**[Chinese|中文文档](README.zh.md)**

A lightweight, multi-source package manager for Linux, macOS, and Windows. Create isolated **environments** and install packages from major software distributions (RPM, DEB, Alpine, Arch, Conda, Homebrew, MSYS2) without root. Each environment is tied to a **channel** (e.g. Debian, Fedora, Alpine with version). Register multiple environments to combine their binaries in your PATH and mix software from different distros in one shell.

```yaml
# Conceptually
host: openeuler | centos | debian | ...
  env1: openeuler   → PATH += $env_root/ebin
  env2: ubuntu      → PATH += $env_root/ebin
```

## Use cases

- **End users** — Install and run software from multiple sources.
- **Developers** — Define project dependencies in one environment (OS packages + language runtimes); reproducible, isolated envs.
- **Containers / embedded** — Replace dnf/apt/apk/pacman with a smaller footprint (~100MB less for RPM, ~20MB for DEB) and optional busybox-style applets.
- **Local AI / sandbox** — Lightweight environment for development and tools that need an isolated filesystem view.

## Features

- **User-space installs** — No root required for normal use.
- **Multi-distro support** — openEuler, Fedora, Debian, Ubuntu, Alpine, Arch Linux, AUR, Conda, Homebrew, MSYS2.
- **Environment isolation** — Per-env channels; register multiple envs and combine their binaries in PATH.
- **Efficient** — File-level deduplication, parallel/chunked downloads, ~1300 global mirrors, fast list/search (e.g. 17x faster than dnf).
- **Portable** — Static musl binary (~14MB); package manager with busybox-style applets to support distroless RPM/DEB/APK containers.
- **Reliable** — SAT-based dependency resolution (resolvo), transaction history with rollback.

## Quick start

**Linux, macOS, Windows WSL2:**
```bash
curl -fsSL https://raw.atomgit.com/openeuler/epkg/raw/master/bin/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.atomgit.com/openeuler/epkg/raw/master/bin/install.ps1 | iex
```

Then start a new shell so PATH is updated.

```bash
# Create an environment and install/run packages
epkg env create alpine -c alpine
epkg -e alpine install htop bash
epkg -e alpine run htop
epkg -e alpine run --isolate=env bash  # run with minimal bind mounts (the default mode)
epkg -e alpine run --isolate=fs bash   # run in chroot
epkg -e alpine run --isolate=vm bash   # run in a VM
```

Default environment is `main`. Use `-e ENV` to target another env, or `epkg env register <ENV>` to add an env to your PATH.

## Supported distributions (channels)

- **RPM**: openEuler, Fedora, CentOS, AlmaLinux, Rocky, EPEL, etc.
- **DEB**: Debian, Ubuntu, Linux Mint, Deepin, etc.
- **Alpine**: main, community
- **Arch**: core, extra, multilib, AUR, arch4edu, etc.
- **Conda**: conda-forge, main, free, and others
- **Homebrew**: Bottle binary packages only ; usable in macOS and Linux
- **MSYS2**: Windows only

Run `epkg repo list` to see the full channel table.

## Main commands (overview)

| Area | Commands |
|------|----------|
| Self | `self install`, `self upgrade`, `self remove` |
| Packages | `install`, `update`, `upgrade`, `remove`, `list`, `info`, `search` |
| Environments | `env list`, `create`, `remove`, `register`, `unregister`, `activate`, `deactivate`, `export`, `path`, `config` |
| VM | `vm start`, `stop`, `list`, `status` |
| History | `history`, `restore <gen_id>` |
| Execute | `run`, `service`, `busybox` |
| Other | `gc`, `repo list`, `hash`, `unpack`, `convert` |

See [Command reference](docs/en/reference/commands.md) for full help.

## Installation layout

- **User-private**: `~/.epkg/envs`, `~/.epkg/store`, and under `~/.cache/epkg/`: `downloads/`, `channels/`, `aur_builds/`, `iploc/`.
- **Shared (root)**: `/opt/epkg` (cache/, store/; envs under `/opt/epkg/envs/root/`).

Details: [Paths and layout](docs/en/reference/paths.md).

## How it works (brief)

- **`epkg run`** offers three isolation modes: `--isolate=env` (default, mount + user namespaces with bind-mounts), `--isolate=fs` (chroot-based isolation), and `--isolate=vm` (VM-based isolation).
- **Install flow**: Resolve (SAT solver) → Download+Unpack → Link (store → env) → Scriptlets → Triggers → Expose binaries to `ebin/` for PATH.

## Build from source

```bash
git clone https://atomgit.com/openeuler/epkg
cd epkg
make dev-depends
make
target/debug/epkg self install
# Then: start a new shell and try out epkg
```

Full dev setup and testing: [Developer quick start](docs/en/user-guide/developer-quick-start.md).

## Documentation

| Document | Description |
|----------|-------------|
| [Documentation index](docs/en/index.md) | Overview and links to all docs |
| [Developer quick start](docs/en/user-guide/developer-quick-start.md) | Build from source, dev loop, testing |
| [Getting started](docs/en/user-guide/getting-started.md) | Installation and first steps |
| [Environments](docs/en/user-guide/environments.md) | Create, register, activate, path, config |
| [Package operations](docs/en/user-guide/package-operations.md) | Install, remove, update, upgrade, list, search, info |
| [Advanced usage](docs/en/user-guide/advanced.md) | run, service, history/restore, gc, convert, unpack |
| [Troubleshooting](docs/en/user-guide/troubleshooting.md) | Common issues and solutions |
| [Command reference](docs/en/reference/commands.md) | All commands and options |
| [Repositories](docs/en/reference/repositories.md) | Channels and repo list |
| [Paths and layout](docs/en/reference/paths.md) | Install directories and layout |

Design notes and format specs: [docs/design-notes/](docs/design-notes/), [docs/epkg-format.md](docs/epkg-format.md).

## Links

- Repository: [atomgit.com/openeuler/epkg](https://atomgit.com/openeuler/epkg)
- Mirrors config: [assets/mirrors/mirrors.json](https://atomgit.com/openeuler/epkg/tree/master/assets/mirrors/mirrors.json)
