# Background_Codex_Quota

Windows x64 Rust backend for reading Codex quota information from a local
`auth.json` file.

## Rust build

Run Cargo with the manually installed absolute path:

```powershell
$cargo = 'C:\code\path\cargo\bin\cargo.exe'
& $cargo fmt --check
& $cargo clippy --all-targets -- -D warnings
& $cargo test
& $cargo build --release
```

The release binary is `target\release\background_codex_quota.exe`.
The deployed executable is named `Background_Codex_Quota.exe`.

## Project layout

- `src\`: Rust application source
- `src\quota\`: upstream client and quota parser
- `target\`: ignored Cargo build output
- `Background_Codex_Quota.exe`: deployed Windows executable

The default server address is `127.0.0.1:8787`. `CODEX_QUOTA_HOST` and
`CODEX_QUOTA_PORT` can override the bind address and port. Authentication uses
`CODEX_AUTH_JSON`, then `CODEX_HOME\auth.json`, then
`%USERPROFILE%\.codex\auth.json`.

The Rust runtime is headless and does not create a tray icon or its own control
window. Release builds use the GUI subsystem and do not create a console window;
diagnostics are written to stderr when a parent console exists. No log file is
created. A separate local GUI can check status, fetch current quota, and stop the
backend through the HTTP API.

## Endpoints

- `GET /health`
- `GET /quota/usage`
- `POST /control/shutdown`

`/quota/usage` returns used percentages in a fixed weekly, monthly, and extra
shape. The active primary window populates either `weekly` or `monthly`;
unavailable values are `null`. The first valid additional rate limit populates
`extra`.

```json
{
  "weekly": 28,
  "monthly": null,
  "extra": 17
}
```

Authentication failures return `401`, upstream HTTP failures return `502`, and
network or internal processing failures return `500`.

Every `GET /quota/usage` request performs a fresh upstream lookup, so a
separate refresh endpoint is unnecessary. `POST /control/shutdown` returns
`202` with the following response and then gracefully releases the process and
HTTP port:

```json
{
  "status": "shutting_down"
}
```

Access to the shutdown endpoint follows the configured bind address. When
`CODEX_QUOTA_HOST` exposes the backend beyond loopback, restrict access with the
host firewall.

## Stream Deck plugin

The Stream Deck source is not included in this repository. Consumers must use
`GET /quota/usage` and interpret values as percentages already used.
