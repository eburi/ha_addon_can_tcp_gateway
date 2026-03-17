"""Tests for the Python CAN-to-TCP gateway (src/python/gateway.py).

All tests avoid real SocketCAN hardware by mocking the `can` module where
necessary and exercising the pure formatting / parsing logic directly.
"""

import asyncio
import re
import unittest
from unittest.mock import AsyncMock, MagicMock, patch

import can

from gateway import (
    CanRawGateway,
    encode_raw_received,
    encode_raw_transmit,
    parse_raw_line,
    setup_logging,
    utc_timestamp,
)


# ---------------------------------------------------------------------------
# utc_timestamp
# ---------------------------------------------------------------------------


class TestUtcTimestamp(unittest.TestCase):
    """Verify timestamp format matches YD RAW spec: HH:MM:SS.mmm"""

    TIMESTAMP_RE = re.compile(r"^\d{2}:\d{2}:\d{2}\.\d{3}$")

    def test_format(self):
        ts = utc_timestamp()
        self.assertRegex(ts, self.TIMESTAMP_RE)

    def test_length(self):
        ts = utc_timestamp()
        self.assertEqual(len(ts), 12)  # "HH:MM:SS.mmm"

    def test_returns_string(self):
        ts = utc_timestamp()
        self.assertIsInstance(ts, str)


# ---------------------------------------------------------------------------
# encode_raw_received / encode_raw_transmit
# ---------------------------------------------------------------------------


def _make_can_msg(arb_id: int, data: bytes) -> can.Message:
    return can.Message(
        arbitration_id=arb_id,
        data=data,
        is_extended_id=True,
        dlc=len(data),
    )


class TestEncodeRawReceived(unittest.TestCase):
    def test_basic_frame(self):
        msg = _make_can_msg(0x19F51323, bytes([0x01, 0x02, 0x03, 0x04]))
        raw = encode_raw_received(msg)
        self.assertIsInstance(raw, bytes)
        text = raw.decode("ascii")
        self.assertTrue(text.endswith("\r\n"))
        self.assertIn(" R ", text)
        self.assertIn("19F51323", text)
        self.assertIn("01 02 03 04", text)

    def test_empty_data(self):
        msg = _make_can_msg(0x00000001, b"")
        raw = encode_raw_received(msg)
        text = raw.decode("ascii")
        self.assertIn(" R ", text)
        self.assertIn("00000001", text)
        # No trailing data bytes — just ID after direction
        parts = text.strip().split()
        # timestamp R CANID
        self.assertEqual(len(parts), 3)

    def test_single_byte(self):
        msg = _make_can_msg(0x09F805FD, bytes([0xFF]))
        raw = encode_raw_received(msg)
        text = raw.decode("ascii")
        self.assertIn("FF", text)

    def test_full_8_bytes(self):
        msg = _make_can_msg(0x1FFFFFFF, bytes(range(8)))
        raw = encode_raw_received(msg)
        text = raw.decode("ascii")
        self.assertIn("00 01 02 03 04 05 06 07", text)


class TestEncodeRawTransmit(unittest.TestCase):
    def test_direction_tag(self):
        msg = _make_can_msg(0x19F51323, bytes([0xAA, 0xBB]))
        raw = encode_raw_transmit(msg)
        text = raw.decode("ascii")
        self.assertIn(" T ", text)
        self.assertNotIn(" R ", text)

    def test_crlf_terminator(self):
        msg = _make_can_msg(0x00000001, b"\x00")
        raw = encode_raw_transmit(msg)
        self.assertTrue(raw.endswith(b"\r\n"))


# ---------------------------------------------------------------------------
# parse_raw_line
# ---------------------------------------------------------------------------


class TestParseRawLine(unittest.TestCase):
    def test_bare_id_and_data(self):
        msg = parse_raw_line("19F51323 01 02 03 04")
        self.assertIsNotNone(msg)
        self.assertEqual(msg.arbitration_id, 0x19F51323)
        self.assertEqual(list(msg.data), [0x01, 0x02, 0x03, 0x04])
        self.assertTrue(msg.is_extended_id)

    def test_with_timestamp_and_direction(self):
        msg = parse_raw_line("12:30:15.482 R 19F51323 01 02 03 04")
        self.assertIsNotNone(msg)
        self.assertEqual(msg.arbitration_id, 0x19F51323)
        self.assertEqual(list(msg.data), [0x01, 0x02, 0x03, 0x04])

    def test_with_timestamp_and_T(self):
        msg = parse_raw_line("00:00:00.000 T 09F805FD FF")
        self.assertIsNotNone(msg)
        self.assertEqual(msg.arbitration_id, 0x09F805FD)
        self.assertEqual(list(msg.data), [0xFF])

    def test_direction_only(self):
        msg = parse_raw_line("R 19F51323 AA BB")
        self.assertIsNotNone(msg)
        self.assertEqual(msg.arbitration_id, 0x19F51323)

    def test_empty_string(self):
        self.assertIsNone(parse_raw_line(""))

    def test_whitespace_only(self):
        self.assertIsNone(parse_raw_line("   "))

    def test_invalid_hex(self):
        self.assertIsNone(parse_raw_line("ZZZZZZZZ 01"))

    def test_no_data_bytes(self):
        msg = parse_raw_line("19F51323")
        self.assertIsNotNone(msg)
        self.assertEqual(msg.arbitration_id, 0x19F51323)
        self.assertEqual(len(msg.data), 0)

    def test_timestamp_only_returns_none(self):
        """Timestamp + direction but no CAN ID => None."""
        self.assertIsNone(parse_raw_line("12:30:15.482 R"))

    def test_roundtrip_received(self):
        """encode_raw_received -> parse_raw_line should recover the frame."""
        original = _make_can_msg(0x19F51323, bytes([0x01, 0x02, 0x03]))
        encoded = encode_raw_received(original).decode("ascii")
        parsed = parse_raw_line(encoded)
        self.assertIsNotNone(parsed)
        self.assertEqual(parsed.arbitration_id, original.arbitration_id)
        self.assertEqual(list(parsed.data), list(original.data[: original.dlc]))

    def test_roundtrip_transmit(self):
        """encode_raw_transmit -> parse_raw_line should recover the frame."""
        original = _make_can_msg(0x09F805FD, bytes([0xFF, 0x00]))
        encoded = encode_raw_transmit(original).decode("ascii")
        parsed = parse_raw_line(encoded)
        self.assertIsNotNone(parsed)
        self.assertEqual(parsed.arbitration_id, original.arbitration_id)
        self.assertEqual(list(parsed.data), list(original.data[: original.dlc]))


# ---------------------------------------------------------------------------
# setup_logging
# ---------------------------------------------------------------------------


class TestSetupLogging(unittest.TestCase):
    def test_accepts_valid_levels(self):
        for level in ("debug", "info", "warning", "error"):
            setup_logging(level)  # should not raise

    def test_unknown_level_defaults(self):
        setup_logging("bogus")  # should not raise


# ---------------------------------------------------------------------------
# CanRawGateway construction
# ---------------------------------------------------------------------------


class TestCanRawGatewayInit(unittest.TestCase):
    @patch.dict(
        "os.environ",
        {
            "CAN_INTERFACE": "vcan0",
            "LISTEN_HOST": "127.0.0.1",
            "LISTEN_PORT": "3000",
        },
    )
    def test_reads_env(self):
        gw = CanRawGateway()
        self.assertEqual(gw.can_interface, "vcan0")
        self.assertEqual(gw.host, "127.0.0.1")
        self.assertEqual(gw.port, 3000)

    @patch.dict("os.environ", {}, clear=True)
    def test_defaults(self):
        gw = CanRawGateway()
        self.assertEqual(gw.can_interface, "can0")
        self.assertEqual(gw.host, "0.0.0.0")
        self.assertEqual(gw.port, 2598)

    def test_initial_state(self):
        gw = CanRawGateway()
        self.assertIsNone(gw.bus)
        self.assertIsNone(gw.reader)
        self.assertIsNone(gw.notifier)
        self.assertEqual(len(gw.clients), 0)


# ---------------------------------------------------------------------------
# CanRawGateway._drop_client
# ---------------------------------------------------------------------------


class TestDropClient(unittest.TestCase):
    def test_drop_removes_and_closes(self):
        gw = CanRawGateway()
        writer = AsyncMock()
        writer.close = MagicMock()
        writer.wait_closed = AsyncMock()
        gw.clients.add(writer)
        self.assertIn(writer, gw.clients)

        asyncio.get_event_loop().run_until_complete(gw._drop_client(writer))
        self.assertNotIn(writer, gw.clients)
        writer.close.assert_called_once()

    def test_drop_nonexistent_is_safe(self):
        gw = CanRawGateway()
        writer = AsyncMock()
        writer.close = MagicMock()
        writer.wait_closed = AsyncMock()
        # Should not raise even though writer was never added
        asyncio.get_event_loop().run_until_complete(gw._drop_client(writer))


if __name__ == "__main__":
    unittest.main()
