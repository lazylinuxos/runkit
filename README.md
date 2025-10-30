# Runkit

Graphical manager for Void Linux runit services. The application targets a friendly, guided user experience that balances power-user workflows with newcomers who just want to start, stop, or understand system services.

## Workspace Layout

- `runkit-core`: service discovery, status parsing, and shared domain types.
- `runkitd`: privileged helper invoked through `pkexec`; executes `sv` commands and manages the `/var/service` symlinks in a controlled manner.
- `runkit`: libadwaita interface that lists services, provides detail panes, and delegates every privileged operation (including status reads) to `runkitd`.

## Building

This workspace requires the Rust 1.83+ toolchain (edition 2024). The GTK frontend also depends on system libraries:

```bash
# Void Linux
sudo xbps-install -S rustup gtk4-devel libadwaita-devel glib-devel pango-devel pkg-config
rustup default stable
```

Once dependencies are present:

```bash
cargo build                # builds every crate
cargo build -p runkit-core # builds just the core for faster iteration
```

> **Note:** `cargo check -p runkit` (or a full `cargo build`) will fail unless the GTK/libadwaita headers are installed. The helper and core crates can be compiled independently with standard Rust tooling.

## Installation

For Void Linux the repository ships an installer that builds release binaries and places them under `/usr/libexec`:

```bash
./install.sh           # installs dependencies, builds, and installs binaries
./install.sh uninstall # removes the installed binaries
```

Run the script from the repository root. It will prompt for sudo once to complete package installation and file placement.

## Running The App

During development you can bypass `pkexec` and point the UI at a locally built helper:

```bash
cargo build --bins
RUNKITD_PATH=target/debug/runkitd \
RUNKITD_NO_PKEXEC=1 \
  cargo run -p runkit
```

When running normally, `runkit` will invoke the helper for **all** service discovery and lifecycle work, so the first launch will trigger a polkit password prompt. The helper is launched via:

```bash
pkexec /usr/libexec/runkitd <action> <service>
```

so ensure your helper binary and accompanying polkit policy are installed at those paths for production.

### Environment Overrides

The desktop app looks for the following overrides when spawning `runkitd`:

- `RUNKITD_PATH`: full path to the helper binary (defaults to `/usr/libexec/runkitd`).
- `RUNKITD_NO_PKEXEC`: set to `1`/`true` to bypass `pkexec` (useful in development environments).

The legacy `RUNKIT_HELPER_PATH` / `RUNKIT_HELPER_NO_PKEXEC` variables are still honored for compatibility.

## Polkit Daemon Policy

The helper currently expects a polkit policy similar to:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<policyconfig>
  <action id="org.voidlinux.runkit.manage">
    <description>Manage Void Linux runit services</description>
    <message>Authentication is required to manage runit services</message>
    <defaults>
      <allow_active>auth_admin_keep</allow_active>
      <allow_any>no</allow_any>
      <allow_inactive>no</allow_inactive>
    </defaults>
  </action>
</policyconfig>
```

Install it under `/usr/share/polkit-1/actions/org.voidlinux.runkit.policy` and ensure the helper binary is placed in `/usr/libexec/runkitd` with root ownership and `0755` permissions.

## Next Steps

- Persist service refresh timers and live updates (inotify/watch the supervise sockets).
- Tail structured logs through the helper and surface them in the UI log tab.
- Expand helper commands to surface richer error details back to the UI (exit status, stderr snippets).
