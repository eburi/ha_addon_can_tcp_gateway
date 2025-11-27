# Home Assistant Add-on: CAN to TCP Gateway

This add-on exposes a SocketCAN interface (e.g. `can0`) as a TCP stream
using a simple NMEA 2000-style binary framing:

- 4 bytes CAN ID (big-endian)
- 1 byte DLC
- 0–8 bytes data payload

This allows you to connect SignalK (or other tooling) over TCP and treat
it similarly to a Yacht Devices NMEA 2000 bridge on `localhost`.

> **Note:** The exact Yacht Devices YDEN-02 TCP binary format may contain
> additional fields (timestamps, channel IDs, etc.). This add-on uses a
> minimal, generic binary framing; adjust `encode_frame_simple()` in
> `can-tcp-gateway/gateway.py` if you need an exact binary match.

## Installation

1. Create a new GitHub repository, e.g. `ha-addon-can-tcp-gateway`.
2. Put this repository’s contents in it.
3. In Home Assistant:
   - Go to **Settings → Add-ons → Add-on Store**.
   - Click the ⋮ menu → **Repositories**.
   - Add your repository URL, e.g.  
     `https://github.com/<YOUR_GITHUB_USERNAME>/ha-addon-can-tcp-gateway`.
4. The **CAN to TCP Gateway** add-on should appear in the list.
5. Install and start the add-on.

## Configuration

Options (set in the add-on UI):

- `can_interface` (string, default: `can0`): the SocketCAN interface.
- `listen_host` (string, default: `0.0.0.0`): TCP listen address.
- `listen_port` (int, default: `2598`): TCP port for clients.
- `log_level` (`info`, `debug`, `warning`, `error`): log verbosity.

## SignalK Setup

In the SignalK Add-on or your SignalK instance:

1. Add a TCP NMEA2000 / binary data source.
2. Point it to:
   - Host: `127.0.0.1`
   - Port: `2598` (or whatever you configured)
3. Restart SignalK.

## Notes

- The add-on runs in `host_network` mode and requires `NET_ADMIN`
  privileges to access the CAN interface.
- Ensure the `can0` interface is already configured and up on the host,
  as Home Assistant OS does not configure it automatically.
