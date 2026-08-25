# Background_Codex_Quota Developer Guide

## Goals and supported scope

This project converts the Codex quota upstream response into a small Windows
x64 background executable and exposes it through a local HTTP API.

The primary design goals are:

- A single Windows x64 executable
- A headless process with no system tray icon or application window
- A stable JSON response shape
- HTTP control for health checks, quota queries, and graceful shutdown
- Diagnostics written only to `stderr`, with no log files
- No tokens or account IDs in logs

The Stream Deck plugin source is not included in this repository. API consumers
must use `GET /quota/usage` and interpret numeric values as percentages used.

## Directory structure

```text
Background_Codex_Quota/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── Background_Codex_Quota.exe
├── docs/
│   └── DEVELOPER_GUIDE.md
└── src/
    ├── main.rs
    ├── api.rs
    ├── auth.rs
    ├── config.rs
    ├── control.rs
    ├── runtime.rs
    ├── windows.rs
    └── quota/
        ├── mod.rs
        ├── client.rs
        └── parser.rs
```

`target/` contains Cargo build output and is excluded from Git. The root-level
`Background_Codex_Quota.exe` is the deployed executable.

## Module responsibilities

| Module | Responsibility |
| --- | --- |
| `main` | Configures the Windows GUI subsystem and handles top-level errors. |
| `config` | Parses bind IP and port environment variables. |
| `auth` | Resolves `auth.json` and extracts the token and account ID. |
| `quota::client` | Builds authentication headers, sends upstream requests, and classifies errors. |
| `quota::parser` | Converts upstream JSON into the stable quota usage response. |
| `api` | Defines Axum routes and HTTP success and error responses. |
| `control` | Shares the process shutdown signal. |
| `runtime` | Owns the single-threaded Tokio runtime, server lifecycle, and graceful shutdown. |
| `windows` | Prevents duplicate instances through a Windows named mutex. |

## HTTP API contract

The default bind address is `127.0.0.1:8787`.

### `GET /health`

- Success status: `200 OK`
- Response:

```json
{"status":"ok"}
```

### `GET /quota/usage`

- Success status: `200 OK`
- Response shape:

```json
{
  "weekly": 28,
  "monthly": null,
  "extra": 17,
  "hourly": 12,
  "code_review": null
}
```

All numeric values represent the percentage **used**, not the percentage
remaining. Every field is an integer in `0..=100` or `null`. Each request
performs a fresh upstream query. `extra` remains reserved for the first valid
item in `additional_rate_limits`.

The legacy `/quota/remaining` endpoint is not available. API consumers and test
fixtures must use `/quota/usage` and follow its usage-percentage semantics.

### `POST /control/shutdown`

- Success status: `202 Accepted`
- Response:

```json
{"status":"shutting_down"}
```

After the response is sent, the shared shutdown signal wakes Axum's graceful
shutdown future. The server then releases its listener and named mutex.

### Error mapping

| Condition | HTTP status | Response shape |
| --- | --- | --- |
| Authentication file or token problem | `401 Unauthorized` | `{"detail":"..."}` |
| Non-success upstream HTTP status | `502 Bad Gateway` | A `detail` object containing the upstream status and reason |
| Network, JSON, or internal processing failure | `500 Internal Server Error` | `{"detail":"quota request failed"}` |

Detailed internal errors are written to `stderr`. Bearer tokens and account IDs
must never be logged.

## Configuration contract

| Environment variable | Default | Rule |
| --- | --- | --- |
| `CODEX_QUOTA_HOST` | `127.0.0.1` | Must be a valid IP address |
| `CODEX_QUOTA_PORT` | `8787` | Must be in `1..=65535` |
| `CODEX_AUTH_JSON` | Not set | Explicit path to an `auth.json` file |
| `CODEX_HOME` | Not set | Uses `auth.json` within this directory |

Authentication paths are resolved in this order:

1. `CODEX_AUTH_JSON`
2. `CODEX_HOME\auth.json`
3. `%USERPROFILE%\.codex\auth.json`

Leading `~` components in `CODEX_AUTH_JSON` and `CODEX_HOME` are expanded to
the user home directory.

Access token fields are checked in this order:

1. `tokens.access_token`
2. `tokens.accessToken`
3. Top-level `access_token`
4. Top-level `accessToken`

Account IDs support the same nested or top-level and snake_case or camelCase
combinations. The access token is sent as `Authorization: Bearer ...`. When
present, the account ID is sent in the `ChatGPT-Account-Id` header.

## Quota parser contract

The upstream URL is `https://chatgpt.com/backend-api/wham/usage`, and the HTTP
timeout is 20 seconds. A response whose top level is not a JSON object is an
internal processing error.

The parser supports both snake_case and camelCase field names:

- `rate_limit` / `rateLimit`
- `primary_window` / `primaryWindow`
- `secondary_window` / `secondaryWindow`
- `used_percent` / `usedPercent`
- `limit_window_seconds` / `limitWindowSeconds`
- `additional_rate_limits` / `additionalRateLimits`
- `code_review_rate_limit` / `codeReviewRateLimit`

Primary window classification:

| Duration | Classification |
| --- | --- |
| At least 25 days | `monthly` |
| At least 6 days but less than 25 days | `weekly` |
| At most 6 hours | `hourly` |
| Any other positive finite duration | `custom` |
| Missing, non-positive, or non-finite | `unknown` |

Primary and secondary windows are evaluated in that order. The first window
matching each period populates the corresponding field, so primary wins when
both windows have the same classification. `weekly`, `monthly`, and `hourly`
are exposed; `custom` and `unknown` remain internal classifications.

For `additional_rate_limits`, the first item containing a valid `used_percent`
becomes `extra`. Percentages use round-half-even to match Python's `round`, then
are clamped to `0..=100`. For example, both `27.5` and `28.5` become `28`.

`code_review_rate_limit` is parsed separately into `code_review`. Its primary
window is preferred, with its secondary window as fallback. It is not merged
into `extra`.

## Runtime and Windows behavior

- Named mutex: `Local\Background_Codex_Quota`
- Second instance: exits immediately without an error
- Tokio runtime: `new_current_thread`
- Shutdown: HTTP shutdown signal and Axum graceful shutdown
- Release subsystem: Windows GUI subsystem
- Logging: `stderr` only, with no log files
- Tray icon, message box, and control window: none

The shutdown API has no application-level authentication. Deployments bound to
an address other than loopback must use Windows Firewall to restrict allowed
source addresses and ports.

## Dependencies and release optimization

The main runtime dependencies are Axum, Tokio, Reqwest, Serde, Thiserror, and
`windows-sys`. Reqwest uses `rustls-tls` instead of native TLS. Axum and Tokio
enable only the required features.

The release profile uses the following settings to minimize executable size:

- `opt-level = "z"`
- LTO enabled
- One codegen unit
- Abort on panic
- Symbols stripped

`Cargo.lock` is committed to keep dependency resolution reproducible.

## Build and verification

Run every Cargo command from the project root using the required absolute path:

```powershell
& 'C:\code\path\cargo\bin\cargo.exe' fmt --check
& 'C:\code\path\cargo\bin\cargo.exe' clippy --all-targets -- -D warnings
& 'C:\code\path\cargo\bin\cargo.exe' test
& 'C:\code\path\cargo\bin\cargo.exe' build --release
```

The release artifact is `target\release\background_codex_quota.exe`.

Tests must protect at least these contracts:

- Weekly, monthly, hourly, extra, and code review parsing
- Primary and secondary window precedence
- Missing values, invalid types, and out-of-range percentages
- snake_case and camelCase input
- Round-half-even and clamping boundaries
- Authentication path and supported field precedence
- Malformed Auth JSON and Auth JSON without a token
- Upstream authentication headers and HTTP status mapping
- `/health`, `/quota/usage`, and `/control/shutdown`
- Named mutex duplicate detection and graceful shutdown

## Deployment procedure

1. Pass formatting, Clippy, tests, and the release build.
2. Call `POST /control/shutdown` on the existing process.
3. Verify that the process has exited and released its port.
4. Copy `target\release\background_codex_quota.exe` to the root-level
   `Background_Codex_Quota.exe`.
5. Start the deployed executable.
6. Verify that `GET /health` returns `200`.
7. With a fixture or real Auth environment, verify the status and stable JSON
   shape returned by `GET /quota/usage`.
8. Call the shutdown API and verify again that the process and port are
   released.

PowerShell smoke test:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/health'
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/quota/usage'
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8787/control/shutdown'
```

## API change checklist

- Were success-response field names and nullability changed intentionally?
- Do tests clearly enforce whether values mean used or remaining percentage?
- Were both the user-facing `README.md` and Developer Guide updated?
- Were existing consumers, including external GUIs and Stream Deck, checked?
- Was the shutdown endpoint's network exposure reviewed?
- Are credentials excluded from error responses and logs?
