# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [1.1.0]

### Added

- App logo (250x100px) and icon (128x128px) for the Home Assistant app store.
- Documentation tab (DOCS.md) with usage instructions and configuration reference.
- App store intro (README.md) with concise add-on summary.
- Changelog (CHANGELOG.md) following Keep a Changelog format.
- TCP watchdog health check for automatic restart on failure.
- `boot: manual` to require explicit start by the user.
- Improved app description in config.yaml.

## [1.0.0]

### Added

- Full-duplex CAN-to-TCP gateway with Yacht Devices RAW text protocol support.
- Rust gateway engine (tokio + socketcan) as the default high-performance option.
- Python gateway engine (asyncio + python-can) as a stable fallback.
- Configurable CAN interface, TCP bind address, port, and log level.
- Multi-architecture support (amd64, aarch64).
- Automatic CAN interface setup via `ip link` in the add-on entry point.
