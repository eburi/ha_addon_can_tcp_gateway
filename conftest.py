"""Add src/python/ to sys.path so pytest can import gateway."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "src" / "python"))
