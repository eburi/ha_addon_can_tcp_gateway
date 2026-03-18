# CAN to TCP Gateway

Full-duplex CAN-to-TCP gateway for Home Assistant.

Exposes a Linux SocketCAN interface (e.g. `can0`) as a Yacht Devices RAW
text TCP feed on port 2598, enabling SignalK or other marine software to
consume and transmit NMEA 2000 CAN frames -- no dedicated bridge hardware
required.

Supports both a high-performance **Rust** engine (default) and a **Python**
fallback, selectable via configuration.

For detailed setup instructions and configuration options, see the
**Documentation** tab.
