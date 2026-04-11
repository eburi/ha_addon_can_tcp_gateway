# CAN to TCP Gateway

## How it works

This add-on creates a full-duplex bridge between a Linux SocketCAN interface
(e.g. `can0`) and a TCP server on port 2598. It implements the
**Yacht Devices RAW text protocol** so that marine software such as SignalK can
consume NMEA 2000 CAN frames without dedicated bridge hardware.

Every CAN frame received from the bus is broadcast to all connected TCP clients
with an `R` (Received) direction tag. Frames sent by a TCP client are written
to the CAN bus and echoed back with a `T` (Transmitted) tag, then broadcast to
all other connected clients.

### RAW text format

```
hh:mm:ss.sss R 19F51323 01 02 03 04     <- received from CAN
hh:mm:ss.sss T 19F51323 01 02 03 04     <- transmitted by TCP client
```

## Configuration

| Option           | Description                              | Default    |
|------------------|------------------------------------------|------------|
| `can_interface`  | SocketCAN interface name                 | `can0`     |
| `listen_port`    | TCP listen port                          | `2598`     |
| `log_level`      | Logging verbosity                        | `info`     |
| `gateway_engine` | Gateway implementation (`python`/`rust`) | `rust`     |

The add-on no longer configures CAN bitrate or link state. Configure and bring
up the CAN interface on the host OS before starting this add-on.

### Gateway engine

The add-on ships with two gateway implementations:

- **`rust`** (default) -- high-performance gateway using tokio with dedicated
  CAN I/O threads. Recommended for lower CPU usage under production bus load.
- **`python`** -- asyncio-based gateway. Stable and well-tested fallback.

Change the `gateway_engine` option and restart the add-on to switch.

## Connecting a client

After starting the add-on, connect any TCP client to port **2598** on your
Home Assistant host:

```bash
nc homeassistant.local 2598
```

To transmit a frame to the CAN bus, send the CAN ID and data bytes:

```
19F51323 01 02 03 04
```

The gateway echoes back a timestamped `T` line confirming transmission.

## CAN hardware setup

This add-on requires a working SocketCAN interface. If you are running
Home Assistant OS on a Raspberry Pi with a CAN HAT (e.g. Waveshare 2-CH
CAN HAT+), you need to configure the appropriate device tree overlays in
your `config.txt`. See the project
[README](https://github.com/eburi/ha_addon_can_tcp_gateway) for detailed
hardware setup instructions.

## Support

If you have questions or run into issues, please open an issue on the
[GitHub repository](https://github.com/eburi/ha_addon_can_tcp_gateway/issues).
