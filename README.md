# Background_Codex_Quota

`Background_Codex_Quota` is a lightweight Windows x64 background service that
reads Codex quota usage through local Codex credentials and exposes it over an
HTTP API for GUI applications and automation tools.

For implementation, build, test, and deployment details, see the
[Developer Guide](docs/DEVELOPER_GUIDE.md).

## Key features

- Runs without a system tray icon or application window.
- Listens on `http://127.0.0.1:8787` by default.
- Reports the percentage **used**, not the percentage remaining.
- Supports health checks, quota queries, and graceful shutdown over HTTP.

## Requirements

You need the following:

- `Background_Codex_Quota.exe`
- An `auth.json` created by signing in to Codex
- An available TCP port `8787`, unless you configure a different port

The service looks for the authentication file in this order and uses the first
configured location:

1. The file specified by `CODEX_AUTH_JSON`
2. `CODEX_HOME\auth.json`
3. `%USERPROFILE%\.codex\auth.json`

Do not share your authentication token. The service does not write the token or
account ID to logs.

## Running the service

Start `Background_Codex_Quota.exe` from File Explorer or PowerShell:

```powershell
& 'C:\Path\To\Background_Codex_Quota.exe'
```

No window or tray icon appears. This is expected. If another instance is
already running in the same Windows session, the new instance exits
immediately.

Check whether the service is running:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/health'
```

A healthy service returns:

```json
{
  "status": "ok"
}
```

## Changing the address or port

The default address is `127.0.0.1:8787`. Set environment variables before
starting the service to change it:

```powershell
$env:CODEX_QUOTA_HOST = '127.0.0.1'
$env:CODEX_QUOTA_PORT = '8787'
& 'C:\Path\To\Background_Codex_Quota.exe'
```

`CODEX_QUOTA_HOST` must be an IP address. `CODEX_QUOTA_PORT` must be between
`1` and `65535`. Restart the service after changing either value.

To use a specific authentication file:

```powershell
$env:CODEX_AUTH_JSON = 'C:\Users\YourName\.codex\auth.json'
& 'C:\Path\To\Background_Codex_Quota.exe'
```

## API usage

### Health check

```http
GET /health
```

The endpoint returns `200 OK` with:

```json
{
  "status": "ok"
}
```

### Quota usage

```http
GET /quota/usage
```

PowerShell example:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/quota/usage'
```

Example response:

```json
{
  "weekly": 28,
  "monthly": null,
  "extra": 17
}
```

Each number is a percentage already used, from `0` through `100`.

- `weekly`: usage of the primary weekly quota
- `monthly`: usage of the primary monthly quota
- `extra`: usage of the first additional limit
- `null`: the corresponding limit is not present in the current response

Only the applicable `weekly` or `monthly` primary field may be populated. Each
request fetches fresh upstream data, so there is no separate refresh endpoint.

### Graceful shutdown

```http
POST /control/shutdown
```

PowerShell example:

```powershell
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8787/control/shutdown'
```

The service returns `202 Accepted` and then releases its HTTP port and exits
gracefully:

```json
{
  "status": "shutting_down"
}
```

## Error responses

| HTTP status | Meaning | What to check |
| --- | --- | --- |
| `401` | Authentication file or token problem | Check the `auth.json` path, sign-in state, and token fields. |
| `502` | The upstream service returned an HTTP error | Retry later or check the Codex service status. |
| `500` | Network, JSON parsing, or internal error | Check network access and diagnostic output. |

Error responses contain a `detail` field. Authentication tokens and account IDs
are never included in API responses.

## Security and remote control

The default host, `127.0.0.1`, accepts connections only from the same computer.
To allow access from another device, set `CODEX_QUOTA_HOST` to an appropriate IP
or bind address and configure Windows Firewall.

The shutdown endpoint has no application-level authentication. When remote
access is enabled, use firewall rules to allow only trusted source IP addresses
and networks.

## Troubleshooting

### Nothing appears after starting the executable

This is expected. The service has no tray icon or application window. Use
`GET /health` to check whether it is running.

### `/health` is unreachable

- Check whether another process is using port `8787`.
- Verify `CODEX_QUOTA_HOST` and `CODEX_QUOTA_PORT`.
- Check whether an existing instance is bound to a different address.
- Review inbound Windows Firewall rules.

### `/quota/usage` returns `401`

- Sign in to Codex again and verify that `auth.json` exists.
- Check whether `CODEX_AUTH_JSON` or `CODEX_HOME` points to an outdated path.
- Verify that the current Windows user can read the selected file.

### No log file is created

This is expected. The service writes diagnostics only to `stderr` and creates no
log file. Diagnostic output is visible only in development or diagnostic runs
that have an attached parent console.

## Updating

Before replacing the executable, call the shutdown endpoint and wait for the
process to exit. Replace `Background_Codex_Quota.exe`, start the new executable,
and verify both `/health` and `/quota/usage`.
