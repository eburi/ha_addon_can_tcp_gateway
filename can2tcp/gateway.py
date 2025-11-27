import asyncio
import logging
import os
import time
from typing import Set

import can


# ---------------------------------------------------------------------------
# Logging & utilities
# ---------------------------------------------------------------------------

def setup_logging(level: str):
    numeric = {
        "debug": logging.DEBUG,
        "info": logging.INFO,
        "warning": logging.WARNING,
        "error": logging.ERROR,
    }.get(level.lower(), logging.INFO)

    logging.basicConfig(
        level=numeric,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s"
    )


def utc_timestamp() -> str:
    """
    Return UTC time as hh:mm:ss.sss (per YD RAW format).
    """
    t = time.time()
    tm = time.gmtime(t)
    ms = int((t - int(t)) * 1000)
    return time.strftime("%H:%M:%S", tm) + f".{ms:03d}"


# ---------------------------------------------------------------------------
# Yacht Devices RAW Formatting
# ---------------------------------------------------------------------------

def encode_raw_received(msg: can.Message) -> bytes:
    """
    Convert a CAN frame into RAW line with 'R' direction.

    Format:
      hh:mm:ss.sss R <CANID> <B0> <B1> ...\r\n
    """
    ts = utc_timestamp()
    msgid = f"{msg.arbitration_id:08X}"
    data_hex = " ".join(f"{b:02X}" for b in msg.data[: msg.dlc])
    line = f"{ts} R {msgid}"
    if data_hex:
        line += " " + data_hex
    line += "\r\n"
    return line.encode("ascii")


def encode_raw_transmit(msg: can.Message) -> bytes:
    """
    RAW line for frames sent by clients ('T' direction).
    """
    ts = utc_timestamp()
    msgid = f"{msg.arbitration_id:08X}"
    data_hex = " ".join(f"{b:02X}" for b in msg.data[: msg.dlc])
    line = f"{ts} T {msgid}"
    if data_hex:
        line += " " + data_hex
    line += "\r\n"
    return line.encode("ascii")


def parse_raw_line(line: str) -> can.Message | None:
    """
    Parse a RAW text line:
      <CANID> <DATA...>

    Timestamp and direction prefixes are ignored if present.
    """
    parts = line.strip().split()
    if not parts:
        return None

    # Strip timestamp if present (HH:MM:SS.mmm)
    if len(parts[0]) == 11 and parts[0][2] == ":" and parts[0][5] == ":" and parts[0][8] == ".":
        parts = parts[1:]

    # Strip direction if present
    if parts and parts[0] in ("R", "T"):
        parts = parts[1:]

    if not parts:
        return None

    # First part must be CAN ID
    try:
        can_id = int(parts[0], 16)
    except ValueError:
        return None

    data = bytes(int(b, 16) for b in parts[1:])
    return can.Message(
        arbitration_id=can_id,
        data=data,
        is_extended_id=True,
        dlc=len(data)
    )


# ---------------------------------------------------------------------------
# TCP ↔ CAN RAW Gateway
# ---------------------------------------------------------------------------

class CanRawGateway:
    def __init__(self):
        self.can_interface = os.environ.get("CAN_INTERFACE", "can0")
        self.host = os.environ.get("LISTEN_HOST", "0.0.0.0")
        self.port = int(os.environ.get("LISTEN_PORT", "2598"))
        self.log = logging.getLogger("CanRawGateway")

        self.bus: can.Bus | None = None
        self.reader: can.AsyncBufferedReader | None = None
        self.notifier: can.Notifier | None = None

        self.clients: Set[asyncio.StreamWriter] = set()
        self.out_queue: asyncio.Queue = asyncio.Queue()

    async def start(self):
        self.log.info(f"Starting CAN RAW gateway on {self.can_interface}")
        self.log.info(f"TCP listening on {self.host}:{self.port}")

        # Start CAN bus
        self.bus = can.Bus(channel=self.can_interface, interface="socketcan")

        # Async CAN reader
        self.reader = can.AsyncBufferedReader()
        loop = asyncio.get_running_loop()
        self.notifier = can.Notifier(self.bus, [self.reader], loop=loop)

        asyncio.create_task(self._can_reader_task())
        asyncio.create_task(self._broadcast_task())

        server = await asyncio.start_server(self._handle_client, self.host, self.port)
        self.log.info("Server ready")

        async with server:
            await server.serve_forever()

    async def _can_reader_task(self):
        """
        Read CAN frames and broadcast to TCP clients as RAW text.
        """
        while True:
            try:
                msg = await self.reader.get_message()
                line = encode_raw_received(msg)
                await self.out_queue.put((line, None))  # None = from CAN
            except Exception:
                self.log.exception("CAN read error")

    async def _broadcast_task(self):
        """
        Send RAW lines to all connected clients.
        """
        while True:
            data, source = await self.out_queue.get()
            dead = []
            for client in list(self.clients):
                if client is source:
                    continue
                try:
                    client.write(data)
                    await client.drain()
                except Exception:
                    dead.append(client)
            for c in dead:
                await self._drop_client(c)

    async def _handle_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        """
        Bi-directional RAW:
        - Receive RAW lines from client → CAN bus → echo back as 'T'
        - Broadcast 'T' frames to other clients
        - Receive CAN frames → broadcast as 'R'
        """
        peer = writer.get_extra_info("peername")
        self.log.info(f"Client connected: {peer}")
        self.clients.add(writer)

        try:
            while True:
                line = await reader.readline()
                if not line:
                    break

                try:
                    text = line.decode("ascii", "ignore").strip()
                    msg = parse_raw_line(text)
                    if msg:
                        # Send to CAN bus
                        self.bus.send(msg)

                        # Echo back to this client
                        echo = encode_raw_transmit(msg)
                        writer.write(echo)
                        await writer.drain()

                        # Broadcast to others
                        await self.out_queue.put((echo, writer))

                except Exception:
                    self.log.exception("Client line error")
        finally:
            self.log.info(f"Client disconnected: {peer}")
            await self._drop_client(writer)

    async def _drop_client(self, writer: asyncio.StreamWriter):
        if writer in self.clients:
            self.clients.remove(writer)
        try:
            writer.close()
            await writer.wait_closed()
        except:
            pass


async def main():
    setup_logging(os.environ.get("LOG_LEVEL", "info"))
    gw = CanRawGateway()
    await gw.start()


if __name__ == "__main__":
    asyncio.run(main())

