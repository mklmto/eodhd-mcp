# EODHD MCP Server

A fast, compiled [MCP](https://modelcontextprotocol.io/) server written in Rust that connects [Claude Desktop](https://claude.ai/download) to the [EODHD](https://eodhd.com/) financial data API — giving Claude direct access to stock prices, company fundamentals, technical indicators, news, macro economic data, and more.

## Features

- **15 tools** covering the full EODHD API surface (34+ endpoints)
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
git clone https://github.com/yourusername/eodhd-mcp.git
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

## Project Structure

```
src/
  main.rs      Entry point — env var loading, stdio server startup
  server.rs    MCP server with all 15 tools (#[tool_router] macro)
  client.rs    HTTP client wrapping all EODHD API endpoints
  types.rs     Parameter structs with JSON Schema generation
  format.rs    Hybrid markdown/JSON output formatting
```

## License

MIT
