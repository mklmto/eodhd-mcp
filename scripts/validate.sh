#!/usr/bin/env bash
# Thin Linux/macOS wrapper around scripts/validate.py.
#
# rmcp's stdio transport shuts down on stdin EOF, so a pure shell harness
# (which closes the pipe after sending requests) loses every response
# after the first. The actual harness is implemented in Python.
#
# Usage:
#   ./scripts/validate.sh
#   EODHD_API_KEY=your-key ./scripts/validate.sh
#   ./scripts/validate.sh --tickers AAPL.US,TSLA.US

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if command -v python3 >/dev/null 2>&1; then
    PY=python3
elif command -v python >/dev/null 2>&1; then
    PY=python
else
    echo "Python 3.9+ is required to run the validator." >&2
    exit 2
fi

exec "$PY" "$SCRIPT_DIR/validate.py" "$@"
