#!/usr/bin/env python3
"""Live smoke harness for the eodhd-mcp server.

Validates the spec section 10 success criterion: a previously 7-call
fundamentals workflow now collapses to `snapshot` + `health_check`
(2 calls). For every reference ticker, both tools are invoked and the
response is checked for the spec section 5.5 envelope tags.

Why Python instead of pure bash/PowerShell: rmcp's stdio transport
treats stdin EOF as a shutdown signal — batch piping (`type ... | bin`)
loses every response after the first. We need to keep stdin open until
all responses arrive, and Windows PowerShell's interactive process I/O
is too brittle for that.

Usage:
  python scripts/validate.py
  EODHD_API_KEY=your-key python scripts/validate.py
  python scripts/validate.py --tickers AAPL.US,TSLA.US
  python scripts/validate.py --bin path/to/eodhd-mcp.exe
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BIN_WIN = REPO_ROOT / "target" / "release" / "eodhd-mcp.exe"
DEFAULT_BIN_NIX = REPO_ROOT / "target" / "release" / "eodhd-mcp"
APPENDIX_B = ["AAPL.US", "ALYA.TO", "SHOP.TO", "GIB.TO", "BRK-A.US"]
DEMO_ALLOWED = {"AAPL.US", "TSLA.US", "VTI.US", "AMZN.US", "BTC-USD.CC", "EURUSD.FOREX"}


def resolve_binary(override: str | None) -> Path:
    if override:
        p = Path(override)
        if not p.exists():
            raise SystemExit(f"binary not found: {p}")
        return p
    for candidate in (DEFAULT_BIN_WIN, DEFAULT_BIN_NIX):
        if candidate.exists():
            return candidate
    print("Binary not found - building release...", file=sys.stderr)
    subprocess.run(["cargo", "build", "--release"], check=True, cwd=REPO_ROOT)
    for candidate in (DEFAULT_BIN_WIN, DEFAULT_BIN_NIX):
        if candidate.exists():
            return candidate
    raise SystemExit("build did not produce expected binary")


def drain(stream, label: str, sink: list[str]) -> None:
    """Background reader so the child never blocks on a full stderr pipe."""
    for line in iter(stream.readline, ""):
        sink.append(line.rstrip())
    stream.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", help="Path to eodhd-mcp executable")
    parser.add_argument(
        "--api-key",
        default=os.environ.get("EODHD_API_KEY", "demo"),
        help="EODHD API key (default: $EODHD_API_KEY or 'demo')",
    )
    parser.add_argument(
        "--tickers",
        help="Comma-separated tickers (default: Appendix B; demo key restricts to AAPL.US)",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=15.0,
        help="Per-tool timeout in seconds (default: 15)",
    )
    args = parser.parse_args()

    binary = resolve_binary(args.bin)

    if args.tickers:
        tickers = [t.strip() for t in args.tickers.split(",") if t.strip()]
    elif args.api_key == "demo":
        print("Demo key in use — restricting to AAPL.US.", file=sys.stderr)
        tickers = ["AAPL.US"]
    else:
        tickers = APPENDIX_B

    env = os.environ.copy()
    env["EODHD_API_KEY"] = args.api_key

    print(f"-> running {len(tickers)} ticker(s) x 2 tool(s) against {binary}")

    # Spawn the server with bidirectional pipes; bufsize=1 = line-buffered.
    proc = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        env=env,
        text=True,
        bufsize=1,
    )

    stderr_sink: list[str] = []
    stderr_thread = threading.Thread(
        target=drain, args=(proc.stderr, "stderr", stderr_sink), daemon=True
    )
    stderr_thread.start()

    next_id = 1

    def send(method: str, params: dict | None = None, notification: bool = False) -> int | None:
        nonlocal next_id
        msg: dict = {"jsonrpc": "2.0", "method": method}
        if not notification:
            msg["id"] = next_id
            mid = next_id
            next_id += 1
        else:
            mid = None
        if params is not None:
            msg["params"] = params
        line = json.dumps(msg, separators=(",", ":"))
        proc.stdin.write(line + "\n")
        proc.stdin.flush()
        return mid

    def recv() -> dict | None:
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            line = proc.stdout.readline()
            if line == "":
                return None  # stdout closed
            line = line.strip()
            if not line:
                continue
            try:
                return json.loads(line)
            except json.JSONDecodeError:
                # Skip non-JSON lines (e.g. accidental tracing output to stdout)
                continue
        return None  # timeout

    # ── Handshake ──
    send("initialize", {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "eodhd-validate", "version": "1.0"},
    })
    init_reply = recv()
    if init_reply is None or init_reply.get("error"):
        print("FAIL: initialize handshake error", file=sys.stderr)
        if stderr_sink:
            print("--- server stderr ---", file=sys.stderr)
            print("\n".join(stderr_sink[-20:]), file=sys.stderr)
        proc.terminate()
        return 2
    send("notifications/initialized", {}, notification=True)

    # ── Per-ticker tool calls ──
    results: list[dict] = []
    envelope_tags = ("<summary>", "</summary>", "<data>", "</data>", "<metadata>", "</metadata>")
    for tic in tickers:
        for tool in ("snapshot", "health_check"):
            t0 = time.monotonic()
            send("tools/call", {"name": tool, "arguments": {"symbol": tic}})
            reply = recv()
            elapsed_ms = int((time.monotonic() - t0) * 1000)
            row = {"ticker": tic, "tool": tool, "elapsed_ms": elapsed_ms}
            if reply is None:
                row["status"] = "FAIL"
                row["detail"] = "timeout / no response"
            elif reply.get("error"):
                row["status"] = "FAIL"
                row["detail"] = reply["error"].get("message", "?")
            else:
                content = reply.get("result", {}).get("content", [])
                body = content[0].get("text", "") if content else ""
                if not body:
                    row["status"] = "FAIL"
                    row["detail"] = "empty content"
                elif not all(tag in body for tag in envelope_tags):
                    row["status"] = "FAIL"
                    row["detail"] = "envelope tags missing"
                else:
                    cache_hit = "yes" if re.search(r"cache_hit:\s+true", body) else "no"
                    row["status"] = "PASS"
                    row["detail"] = f"cache_hit={cache_hit}"
            results.append(row)

    # ── Cleanup ──
    try:
        proc.stdin.close()
    except Exception:
        pass
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.terminate()

    # ── Report ──
    print()
    print(f"{'Ticker':<14} {'Tool':<14} {'Status':<6} {'Time':>8}  Detail")
    print("-" * 70)
    for r in results:
        print(f"{r['ticker']:<14} {r['tool']:<14} {r['status']:<6} {r['elapsed_ms']:>6} ms  {r['detail']}")

    passed = sum(1 for r in results if r["status"] == "PASS")
    total = len(results)
    print()
    if passed == total:
        print(f"[PASS] All {total} checks passed.")
        return 0
    print(f"[FAIL] {total - passed} of {total} checks failed.")
    if stderr_sink:
        print("--- last 10 lines of server stderr ---", file=sys.stderr)
        print("\n".join(stderr_sink[-10:]), file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
