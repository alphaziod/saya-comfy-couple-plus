"""Entry point: python installer/saya.py <install|verify|restore|check-compat|amd> [options]"""
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from saya_installer.cli import main  # noqa: E402

if __name__ == "__main__":
    sys.exit(main())
