# EODHD MCP Server

A fast, compiled [MCP](https://modelcontextprotocol.io/) server written in Rust that connects [Claude Desktop](https://claude.ai/download) to the [EODHD](https://eodhd.com/) financial data API — giving Claude direct access to stock prices, company fundamentals, technical indicators, news, macro economic data, and more.

## Features

- **16 tools** covering the full EODHD API surface (37+ endpoints, including Unicornbay US options)
- **Compiled Rust binary** — fast startup, low memory, no runtime dependencies
- **Hybrid output** — markdown tables for tabular data, JSON for complex/nested responses
- **Windows-native** — builds as a single `.exe`, no OpenSSL required (uses rustls)
- **stdio transport** — standard MCP protocol over stdin/stdout for Claude Desktop

## Requirements

- [Rust toolchain](https://rustup.rs/) (1.70+)
- An [EODHD API key](https://eodhd.com/register) (free demo key available for testing)
- Claude Desktop (or any MCP-compatible client)

## Quick Start

### Build

```bash
git clone https://github.com/mklmto/eodhd-mcp.git
cd eodhd-mcp
cargo build --release
```

The binary is at `target/release/eodhd-mcp.exe` (Windows) or `target/release/eodhd-mcp` (Linux/macOS).

### Configure Claude Desktop

Add the following to your Claude Desktop config file:

- **Windows**: `%APPDATA%\Claude\claude_desktop_config.json`
- **macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "eodhd": {
      "command": "C:\\path\\to\\eodhd-mcp.exe",
      "env": {
        "EODHD_API_KEY": "your-api-key-here"
      }
    }
  }
}
```

Restart Claude Desktop. You should see the EODHD tools available in the tools menu.

### Demo Key

If `EODHD_API_KEY` is not set, the server falls back to the EODHD demo key, which is limited to: `AAPL.US`, `TSLA.US`, `VTI.US`, `AMZN.US`, `BTC-USD.CC`, `EURUSD.FOREX`.

## Tools

### Market Data

| Tool | Description |
|------|-------------|
| `price` | End-of-day historical, intraday (1m/5m/1h), and real-time delayed quotes |
| `fundamentals` | Company financials, highlights, valuation, earnings, ESG — with filter support |
| `dividends_splits` | Dividend payment and stock split history |
| `market_cap` | Historical market capitalization (weekly, US stocks from 2020+) |
| `bulk_data` | Bulk EOD data for an entire exchange in one call |

### Analysis & Screening

| Tool | Description |
|------|-------------|
| `technicals` | 20+ indicators: SMA, EMA, RSI, MACD, Bollinger Bands, ATR, ADX, SAR, CCI, and more |
| `screener` | Screen stocks by market cap, sector, dividend yield, signals, and other criteria |
| `news` | Financial news articles with optional sentiment scores (-1 to +1) |
| `insider_trading` | SEC Form 4 insider buy/sell transactions |
| `options` | US stock options (Unicornbay): EOD Greeks/IV time series, contract discovery, underlyings list — with filters, sparse fieldsets, compact mode, and auto-pagination |

### Economic & Calendar

| Tool | Description |
|------|-------------|
| `macro_economic` | Country-level indicators (GDP, inflation, unemployment) and economic events |
| `calendar` | Earnings, IPOs, splits, dividends, and earnings trend calendars |
| `treasury` | US Treasury rates: T-bill, long-term, par yield curve, real yield |

### Reference

| Tool | Description |
|------|-------------|
| `search` | Find symbols by company name, ticker, or ISIN |
| `exchange_info` | Exchange lists, symbol lists, trading hours and holidays |
| `account` | API usage, subscription type, and remaining quota |

## Symbol Format

EODHD uses `TICKER.EXCHANGE` notation:

| Type | Example |
|------|---------|
| US stocks | `AAPL.US`, `TSLA.US` |
| UK stocks | `VOD.LSE` |
| Crypto | `BTC-USD.CC`, `ETH-USD.CC` |
| Forex | `EURUSD.FOREX` |
| Indices | `GSPC.INDX` |
| Bonds | via `GBOND` exchange |

## Example Prompts

Once configured, you can ask Claude things like:

- *"Show me Apple's stock price for the last 30 days"*
- *"What are the fundamentals highlights for Tesla?"*
- *"Calculate the 14-day RSI for Bitcoin"*
- *"Screen for technology stocks with market cap over $10B and dividend yield above 2%"*
- *"Show me upcoming earnings for this week"*
- *"What's the US GDP trend over the last 10 years?"*
- *"Get the latest financial news about NVIDIA"*
- *"Pull the last 10 days of AAPL 150-strike call options with full Greeks"*
- *"List AAPL option contracts expiring in the next 30 days, strikes 150 to 200"*
- *"How many US underlyings does the Unicornbay options dataset cover?"*

### Options tool notes

The `options` tool wraps the EODHD Unicornbay US Stock Options dataset and **requires an active marketplace subscription** on your EODHD account (the same API key is used — no separate token). Coverage is ~6,000 US underlyings, 2-year EOD history, NASDAQ-routed. The three modes are:

- `eod` — per-contract EOD time series with the full Greek set (delta, gamma, theta, vega, rho), bid/ask/last, volume, open interest, implied volatility.
- `contracts` — lightweight contract discovery (strike, expiry, type) for an underlying.
- `underlyings` — list of covered tickers; takes no filters.

Set `auto_paginate=true` to follow `links.next` automatically (capped by `max_pages`, default 5). Use `fields` for a sparse fieldset and `compact=true` to flatten the JSON:API envelope. The API token is scrubbed from any surfaced pagination URLs.

## Project Structure

```
src/
  main.rs      Entry point — env var loading, stdio server startup
  server.rs    MCP server with all 16 tools (#[tool_router] macro)
  client.rs    HTTP client wrapping all EODHD API endpoints
  types.rs     Parameter structs with JSON Schema generation
  format.rs    Hybrid markdown/JSON output formatting
```

## Disclaimer

This project is an independent, unofficial integration and is **not affiliated with, endorsed by, or sponsored by EODHD (EOD Historical Data)**. It is a thin API client — it does not store, cache, or redistribute any financial data.

**You must have your own EODHD API subscription** to use this server. All data retrieved through this tool is subject to [EODHD's Terms and Conditions](https://eodhd.com/financial-apis/terms-conditions). Your rights to use, display, or redistribute the data depend on your subscription tier (personal, commercial, enterprise). It is your responsibility to comply with EODHD's terms for your plan.

Financial data provided by EODHD is not necessarily real-time nor guaranteed to be accurate. It is not appropriate for trading purposes. The authors of this software bear no responsibility for any trading or investment losses.

## License

MIT — see [LICENSE](LICENSE) for details.
