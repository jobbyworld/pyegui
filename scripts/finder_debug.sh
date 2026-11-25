#!/bin/sh
# Starts a Finder in debug mode, from the terminal (which stays open, the events are written in it):
#   scripts/finder_debug.sh py [folder]    examples/finder.py
#   scripts/finder_debug.sh rs [folder]    finder-rs (target/dist/finder)
# The log is also appended to ~/.local/share/finder-py/debug.log or ~/.local/share/finder-rs/debug.log
# (`--log FILE` to change it, tail -f to follow it from another terminal).
cd "$(dirname "$0")/.." || exit 1
which=${1:-py}
[ $# -gt 0 ] && shift
case "$which" in
  py) exec .venv/bin/python examples/finder.py --debug "$@" ;;
  rs) exec target/dist/finder --debug "$@" ;;
  *) echo "usage: $0 py|rs [folder]" >&2; exit 2 ;;
esac
