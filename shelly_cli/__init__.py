"""
shelly-cli: A fast CLI for discovering, monitoring, and controlling Shelly devices.
"""

try:
    from importlib.metadata import version
    __version__ = version("shelly-cli")
except ImportError:
    from importlib_metadata import version
    __version__ = version("shelly-cli")
