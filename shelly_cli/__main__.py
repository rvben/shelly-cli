"""
Command-line interface for shelly-cli.

Finds the native Rust binary installed by maturin and executes it.
"""

from __future__ import annotations

import os
import shutil
import sys
import subprocess
from pathlib import Path


def find_native_binary() -> str:
    """Find the native shelly binary installed alongside this package."""
    # maturin places the binary in the same bin/ directory as this script
    bin_dir = Path(sys.executable).parent
    for name in ("shelly", "shelly.exe"):
        candidate = bin_dir / name
        if candidate.is_file():
            return str(candidate)

    # Fallback: check PATH
    found = shutil.which("shelly")
    if found:
        return found

    raise FileNotFoundError(
        "Could not find the native shelly binary. "
        "Please ensure shelly-cli is installed correctly."
    )


def main() -> int:
    """Run the shelly command line tool."""
    try:
        native_binary = find_native_binary()
        args = [native_binary] + sys.argv[1:]

        if sys.platform == "win32":
            completed_process = subprocess.run(args)
            return completed_process.returncode
        else:
            os.execv(native_binary, args)
            return 0  # unreachable, but satisfies type checkers
    except FileNotFoundError as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
