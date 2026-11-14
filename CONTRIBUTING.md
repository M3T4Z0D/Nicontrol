# Contributing to Nicontrol

Thank you for your interest in contributing to **Nicontrol**!

## Development Setup

1. Install Rust (`rustup`) and Linux system dependencies (Debian/Ubuntu example):
   ```bash
   sudo apt-get install -y libgl1-mesa-dev libxkbcommon-dev libwayland-dev libdbus-1-dev pkg-config
   ```
2. Run tests and linter before opening a PR:
   ```bash
   cargo test --locked
   cargo clippy --all-targets --locked -- -D warnings
   ```

## Guidelines

* Keep hardware-specific assumptions (model names, EC paths, CPU/GPU vendors) dynamic or behind safe fallbacks.
* Update both `En` and `PtBr` entries in `src/i18n.rs` when adding new UI strings.
* Include your laptop model, kernel version, and EC driver when reporting hardware compatibility issues.
