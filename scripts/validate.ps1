#requires -Version 5.1
<#
.SYNOPSIS
  Thin Windows wrapper around scripts/validate.py.

.DESCRIPTION
  rmcp's stdio transport shuts down on stdin EOF and PowerShell's
  interactive process I/O is fragile on Windows, so the actual harness
  is implemented in Python (scripts/validate.py). This wrapper just
  forwards arguments and ensures Python is on PATH.

.EXAMPLE
  .\scripts\validate.ps1
  .\scripts\validate.ps1 -ApiKey real-key -Tickers AAPL.US,TSLA.US
#>
[CmdletBinding(PositionalBinding=$false)]
param(
    [string]$ApiKey,
    [string]$Tickers,
    [string]$Binary
)

$python = Get-Command python -ErrorAction SilentlyContinue
if (-not $python) { $python = Get-Command python3 -ErrorAction SilentlyContinue }
if (-not $python) {
    Write-Error "Python is required to run the validator. Install Python 3.9+ and try again."
    exit 2
}

$script = Join-Path $PSScriptRoot 'validate.py'
$argList = @($script)

if ($ApiKey)  { $argList += @('--api-key', $ApiKey) }
if ($Tickers) { $argList += @('--tickers', $Tickers) }
if ($Binary)  { $argList += @('--bin', $Binary) }

& $python.Source @argList
exit $LASTEXITCODE
