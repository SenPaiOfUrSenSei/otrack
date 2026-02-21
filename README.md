# otrack-pro (Omarchy Tracker Pro)

High-performance, CLI-native screen time and productivity suite for Hyprland.

## Features
- **Daemon (`otrackd`)**: Background tracker using Hyprland IPC.
- **Deep Work Engine**: Block distracting apps during focus sessions.
- **Grace Period**: 30s delay before logging to avoid noise.
- **CLI (`otrack`)**: Minimalist status and report queries.
- **Dashboard**: TUI dashboard using `ratatui`.
- **Waybar Integration**: JSON tooltip for Waybar status.
- **Privacy First**: Local SQLite DB.

## Installation

### Prerequisites
- Rust & Cargo
- Hyprland
- SQLite (runtime)

### Build
```bash
cargo build --release
```

### Install
```bash
cp target/release/otrack target/release/otrackd ~/.local/bin/
# Or if you have ~/.cargo/bin in your path:
cargo install --path otrack
cargo install --path otrackd
```

### Systemd Setup
```bash
mkdir -p ~/.config/systemd/user/
cp otrackd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now otrackd
```

## Usage
- `otrack status`: Show active app and focus status.
- `otrack report`: Show today's usage summary.
- `otrack start 45`: Start a 45-minute focus session.
- `otrack stop`: End focus session.
- `otrack dashboard`: Launch the TUI dashboard.
- `otrack waybar`: Output JSON for custom Waybar module.

## Configuration
Config is located at `~/.config/otrack/config.toml`.
Edit `blacklist` to specify apps to block during focus sessions.
