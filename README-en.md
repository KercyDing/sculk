# sculk

<kbd>[简体中文](README.md)</kbd>

[![crates.io](https://img.shields.io/crates/v/sculk.svg)](https://crates.io/crates/sculk)
[![docs.rs](https://docs.rs/sculk/badge.svg)](https://docs.rs/sculk)

A P2P tunnel for playing Minecraft over the Internet, built with iroh and QUIC.

The GUI client is available here → [shrieker](https://github.com/KercyDing/shrieker)

> Sculk silently spreads through Minecraft's Deep Dark, carrying signals between its nodes.
>
> sculk works in a similar way: it creates unobtrusive tunnels between players and lets connections happen naturally.

- `sculk`: command-line client
- `sculk`: tunnel core library

## Quick start

### Install with the recommended script

#### macOS / Linux

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.sh)"
```

#### Windows PowerShell

```powershell
& $([scriptblock]::Create((irm https://raw.githubusercontent.com/KercyDing/sculk/main/scripts/install/install.ps1)))
```

See [installation and uninstallation](docs/install.md) for details.

### Basic usage

```sh
# Host a game
sculk host

# Join a game
sculk join "sculk://..."
```

See the [usage guide](docs/usage.md) for details.

## Relay and self-hosting

sculk first attempts to establish a direct P2P connection. When NAT, network policies, or carrier restrictions prevent a direct connection, traffic must be forwarded through a Relay.

The project uses iroh's public Relay infrastructure by default. It works out of the box, but sculk does not control or guarantee its availability, latency, or bandwidth. Connections may therefore be slow, unstable, or unavailable in some regions and complex network environments.

You can deploy a dedicated Relay when you need a more reliable entry point, a server closer to your users, or direct control over bandwidth and availability. Its URL can then be configured in the `sculk` CLI or an application built on sculk.

For deployment instructions and ready-to-use builds, see [iroh-relay](https://github.com/KercyDing/iroh-relay).

## For developers

Development requires Rust `1.91.0` or later and [`only`](https://github.com/KercyDing/only) for project tasks. `cargo-nextest` is optional.

The workspace contains two crates:

- `core`: tunnel core library (`sculk`)
- `cli`: command-line client (`sculk-cli` / `sculk`)

Common commands:

```sh
only check       # Formatting, compilation, and Clippy checks
only ci          # Checks and development tests
only dev build   # Development build
only rel build   # Release build
only install     # Build and install the sculk CLI
```

## License

Copyright (C) 2026 KercyDing

The entire project is dual-licensed under your choice of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
