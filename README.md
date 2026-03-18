# Home Assistant Add-on: CAN to TCP Gateway

<p align="center">
  <img src="logo.svg" alt="CAN to TCP Gateway -- Full-Duplex" width="560"/>
</p>

**Full-duplex CAN-to-TCP gateway implementing the Yacht Devices RAW text protocol (Appendix E).**

Exposes a Linux SocketCAN interface (e.g. `can0`) as a bi-directional TCP feed so
that SignalK, NMEA 2000 tools, or any software expecting the Yacht Devices RAW
interface can consume and transmit CAN frames -- no dedicated hardware bridge required.

Protocol specification: [Yacht Devices YDEN-02 / YDNU-02 Manual -- Appendix E](https://www.yachtd.com/downloads/ydnu02.pdf)

---

## Architecture

```
CAN Bus  <-->  SocketCAN (can0)  <-->  Gateway Add-on  <-->  TCP :2598  <-->  Clients
                                        (full duplex)
```

Every CAN frame received from the bus is broadcast to all connected TCP clients
with an `R` (Received) direction tag. Frames sent by a TCP client are written to
the CAN bus and echoed back with a `T` (Transmitted) tag, then broadcast to all
other connected clients.

### RAW text format

```
hh:mm:ss.sss R 19F51323 01 02 03 04     <- received from CAN
hh:mm:ss.sss T 19F51323 01 02 03 04     <- transmitted by TCP client
```

- Timestamps in UTC with millisecond resolution
- CAN ID in 8-digit uppercase hex
- Data bytes in two-digit uppercase hex, space-separated

---

## Project structure

```
src/
  python/
    gateway.py           -- Python gateway (asyncio + python-can)
  rust/
    Cargo.toml           -- Rust crate manifest
    Cargo.lock
    src/
      main.rs            -- Rust gateway (tokio + socketcan)
tests/
  python/
    test_gateway.py      -- Python unit tests
can2tcp/                 -- Home Assistant Add-on packaging
  config.yaml            -- HA add-on metadata, options, schema
  Dockerfile             -- multi-stage build (Rust + Python)
  run.sh                 -- HA add-on entry point (bashio)
  icon.png               -- HA app icon (128x128)
  logo.png               -- HA app logo (250x100)
  README.md              -- HA app store intro
  DOCS.md                -- HA app documentation tab
  CHANGELOG.md           -- HA app version history
conftest.py              -- adds src/python/ to sys.path for pytest
requirements.txt         -- Python runtime + dev dependencies
local_deploy.sh          -- deploy to a HA device via scp
repository.json          -- HA add-on repository manifest
logo.svg                 -- project logo
.github/workflows/
  ci.yml                 -- lint + test (Python & Rust)
  docker.yml             -- build and publish HA add-on images
```

Source code lives in `src/` with separate directories for each implementation.
The `can2tcp/` directory contains only HA add-on packaging files (Dockerfile,
config.yaml, run.sh). During CI and local deploy, `src/` is copied into the
`can2tcp/` build context so the Dockerfile can access both implementations.

---

## Running without Home Assistant

Both gateway implementations can run standalone on any Linux machine with a
SocketCAN interface. No Home Assistant, no Docker required.

### Python

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install python-can

# Set up a virtual CAN interface for testing
sudo modprobe vcan
sudo ip link add dev vcan0 type vcan
sudo ip link set up vcan0

# Run the gateway
CAN_INTERFACE=vcan0 LISTEN_PORT=2598 python3 src/python/gateway.py
```

### Rust

```bash
cd src/rust
cargo build --release

# Run the gateway
CAN_INTERFACE=vcan0 LISTEN_PORT=2598 ./target/release/can-tcp-gateway
```

### Environment variables

| Variable        | Description              | Default    |
|-----------------|--------------------------|------------|
| `CAN_INTERFACE` | SocketCAN interface name | `can0`     |
| `LISTEN_HOST`   | TCP bind address         | `0.0.0.0`  |
| `LISTEN_PORT`   | TCP listen port          | `2598`     |
| `LOG_LEVEL`     | Logging verbosity        | `info`     |

---

## Running tests

### Python

```bash
pip install -r requirements.txt
pytest tests/python/ -v
```

The `conftest.py` at the project root adds `src/python/` to `sys.path`
automatically so test imports work without installation.

### Rust

```bash
cd src/rust
cargo test --verbose
```

### Linting

```bash
# Python
ruff check src/python/ tests/python/
ruff format --check src/python/ tests/python/

# Rust
cd src/rust
cargo fmt --check
cargo clippy -- -D warnings
```

---

## Home Assistant Add-on installation

1. **Add the repository** to Home Assistant:
   Settings > Add-ons > Add-on Store > Repositories (top-right menu) >
   paste the repository URL.

2. **Install** *CAN to TCP Gateway* from the add-on list.

3. **Start** the add-on.

4. **Connect** your client (SignalK, netcat, etc.):
   ```
   Host: homeassistant.local
   Port: 2598
   ```

### Add-on configuration

| Option           | Description                              | Default    |
|------------------|------------------------------------------|------------|
| `can_interface`  | SocketCAN interface name                 | `can0`     |
| `listen_host`    | TCP bind address                         | `0.0.0.0`  |
| `listen_port`    | TCP listen port                          | `2598`     |
| `log_level`      | Logging verbosity                        | `info`     |
| `gateway_engine` | Gateway implementation (`python`/`rust`) | `rust`     |

### Gateway engine selection

The add-on ships with two gateway implementations:

- **`rust`** (default) -- high-performance rewrite using tokio with dedicated
  CAN I/O threads. Recommended based on lower CPU use under production bus load.
- **`python`** -- asyncio-based gateway. Stable and well-tested.

Change the `gateway_engine` option and restart to switch.

---

## Local deploy to Home Assistant

```bash
./local_deploy.sh [user@host]
# Default: root@192.168.46.222
```

The script assembles a self-contained add-on directory (stripping the `image:`
line from config.yaml so HA builds locally), copies it to the HA device, and
prints instructions for installing/rebuilding.

---

## CI / CD

### CI workflow (`.github/workflows/ci.yml`)

Runs on every push and PR to `main`:

- **Python lint** -- ruff check + format
- **Python test** -- pytest
- **Rust lint** -- cargo fmt + clippy
- **Rust test** -- cargo test

### Docker workflow (`.github/workflows/docker.yml`)

Runs on push to `main` and version tags (`v*`):

- Copies `src/` into the `can2tcp/` build context
- Uses [Home Assistant Builder](https://github.com/home-assistant/builder) to
  produce multi-arch images (`amd64`, `armv7`, `aarch64`)
- Publishes to `ghcr.io/eburi/ha_addon_can_tcp_gateway`

---

## Client examples

### Connect with netcat

```
nc homeassistant.local 2598
```

Sample output:
```
12:41:23.105 R 09F805FD FF 00 00 00
12:41:23.421 R 19F51323 01 02 03 04
```

### Transmit a frame to the CAN bus

```
19F51323 01 02 03 04
```

The gateway echoes back:
```
12:41:30.882 T 19F51323 01 02 03 04
```

---

## CAN HAT setup (Home Assistant OS)

This add-on requires a working SocketCAN interface in HAOS. Below is an example
for the **Waveshare 2-CH CAN HAT+** on a Raspberry Pi 5 with NVMe boot:

```bash
mkdir /mnt/boot
mount -t vfat /dev/nvme0n1p1 /mnt/boot
nano /mnt/boot/config.txt
```

Add:

```
dtparam=spi=on
dtoverlay=i2c0
dtoverlay=spi1-3cs
dtoverlay=mcp2515,spi1-1,oscillator=16000000,interrupt=22
dtoverlay=mcp2515,spi1-2,oscillator=16000000,interrupt=13
```

See the [Waveshare 2-CH CAN HAT+ wiki](https://www.waveshare.com/wiki/2-CH_CAN_HAT+)
for details. Other CAN HATs follow the same dtoverlay pattern.

---

## Roadmap

- Read-only mode option
- Split into two containers (host-network CAN reader + server container exposing TCP)
- Fast-packet reassembly for multi-frame PGNs
- Optional Actisense binary format output

---

## License

See repository for license details.
