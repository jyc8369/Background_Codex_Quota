# Background_Codex_Quota 개발자 가이드

## 목표와 지원 범위

이 프로젝트는 Codex 쿼터 upstream 응답을 작은 Windows x64 백그라운드
실행 파일로 변환해 로컬 HTTP API로 제공합니다.

주요 설계 목표는 다음과 같습니다.

- Windows x64 단일 실행 파일
- 시스템 트레이와 자체 GUI가 없는 headless 프로세스
- 고정된 JSON 응답 구조
- 외부 GUI가 HTTP로 상태 확인, 쿼터 조회, 정상 종료 가능
- 파일 로그 없이 `stderr`만 사용
- 토큰과 계정 ID를 로그에 노출하지 않음

Stream Deck 플러그인 소스는 이 저장소에 포함되지 않습니다. API 소비자는
`GET /quota/usage`를 사용하고 숫자를 사용 비율로 해석해야 합니다.

## 디렉터리 구조

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

`target/`은 Cargo 빌드 산출물이며 Git에서 제외합니다. 루트의
`Background_Codex_Quota.exe`는 실제 배포용 실행 파일입니다.

## 모듈 책임

| 모듈 | 책임 |
| --- | --- |
| `main` | Windows GUI subsystem 설정과 최상위 오류 처리 |
| `config` | bind IP와 포트 환경변수 해석 |
| `auth` | `auth.json` 경로 결정, 토큰과 계정 ID 추출 |
| `quota::client` | 인증 헤더 구성, upstream 요청, 오류 분류 |
| `quota::parser` | upstream JSON을 고정된 사용률 응답으로 변환 |
| `api` | Axum 라우팅, 성공·오류 HTTP 응답 |
| `control` | 프로세스 종료 신호 공유 |
| `runtime` | 단일 스레드 Tokio 런타임, 서버 lifecycle, graceful shutdown |
| `windows` | Windows named mutex를 이용한 중복 실행 방지 |

## HTTP API 계약

기본 bind 주소는 `127.0.0.1:8787`입니다.

### `GET /health`

- 성공 상태: `200 OK`
- 응답:

```json
{"status":"ok"}
```

### `GET /quota/usage`

- 성공 상태: `200 OK`
- 응답 구조:

```json
{
  "weekly": 28,
  "monthly": null,
  "extra": 17
}
```

모든 숫자는 남은 비율이 아니라 **사용한 비율**입니다. 각 필드는 정수
`0..=100` 또는 `null`입니다. 호출할 때마다 upstream을 새로 조회합니다.

기존의 `/quota/remaining` endpoint는 제공하지 않습니다. API 소비자와
테스트 fixture는 `/quota/usage` 및 사용률 의미에 맞춰야 합니다.

### `POST /control/shutdown`

- 성공 상태: `202 Accepted`
- 응답:

```json
{"status":"shutting_down"}
```

응답을 보낸 뒤 공유 종료 신호가 Axum의 graceful shutdown future를 깨우고,
서버가 listener와 named mutex를 해제합니다.

### 오류 매핑

| 조건 | HTTP 상태 | 응답 형태 |
| --- | --- | --- |
| 인증 파일·토큰 문제 | `401 Unauthorized` | `{"detail":"..."}` |
| upstream 비성공 HTTP 상태 | `502 Bad Gateway` | upstream 상태와 reason을 포함한 `detail` 객체 |
| 네트워크·JSON·내부 처리 문제 | `500 Internal Server Error` | `{"detail":"quota request failed"}` |

내부 오류의 구체적인 내용은 `stderr`에 기록하지만 bearer token과 account ID는
기록하지 않습니다.

## 설정 계약

| 환경변수 | 기본값 | 규칙 |
| --- | --- | --- |
| `CODEX_QUOTA_HOST` | `127.0.0.1` | 문자열 IP 주소만 허용 |
| `CODEX_QUOTA_PORT` | `8787` | `1..=65535` |
| `CODEX_AUTH_JSON` | 없음 | `auth.json` 파일의 명시적 경로 |
| `CODEX_HOME` | 없음 | 내부의 `auth.json` 사용 |

Auth 경로 우선순위는 다음과 같습니다.

1. `CODEX_AUTH_JSON`
2. `CODEX_HOME\auth.json`
3. `%USERPROFILE%\.codex\auth.json`

`CODEX_AUTH_JSON`과 `CODEX_HOME`의 선행 `~`는 사용자 홈으로 확장합니다.

지원하는 access token 필드는 다음 순서입니다.

1. `tokens.access_token`
2. `tokens.accessToken`
3. 최상위 `access_token`
4. 최상위 `accessToken`

account ID도 동일하게 중첩/최상위 및 snake_case/camelCase 조합을 지원합니다.
access token은 `Authorization: Bearer ...`, account ID는 존재할 때
`ChatGPT-Account-Id` 헤더로 전송합니다.

## Quota 파서 계약

upstream URL은 `https://chatgpt.com/backend-api/wham/usage`이며 HTTP timeout은
20초입니다. 응답 최상위가 JSON 객체가 아니면 내부 처리 오류입니다.

필드명은 다음 snake_case와 camelCase를 모두 지원합니다.

- `rate_limit` / `rateLimit`
- `primary_window` / `primaryWindow`
- `used_percent` / `usedPercent`
- `limit_window_seconds` / `limitWindowSeconds`
- `additional_rate_limits` / `additionalRateLimits`

기본 window 분류 규칙:

| 기간 | 분류 |
| --- | --- |
| 25일 이상 | `monthly` |
| 6일 이상 25일 미만 | `weekly` |
| 6시간 이하 | `hourly` |
| 그 외 | `custom` |
| 누락, 0 이하, 유한하지 않은 값 | `unknown` |

primary window는 `weekly`와 `monthly`만 API 응답에 채웁니다. `hourly`,
`custom`, `unknown`은 현재 고정 응답 구조에 노출하지 않습니다.

`additional_rate_limits`에서는 올바른 `used_percent`를 가진 첫 번째 항목을
`extra`로 사용합니다. 사용률은 Python의 `round`와 맞춘 round-half-even으로
정수화한 뒤 `0..=100`으로 제한합니다. 예를 들어 `27.5`와 `28.5`는 모두
`28`입니다.

## Runtime과 Windows 동작

- named mutex: `Local\Background_Codex_Quota`
- 두 번째 인스턴스: 오류 없이 즉시 종료
- Tokio runtime: `new_current_thread`
- 종료 방식: HTTP 종료 신호와 Axum graceful shutdown
- release subsystem: Windows GUI subsystem
- 로그: 파일 저장 없이 `stderr`
- tray, 메시지 박스, 자체 제어 창: 없음

종료 API에는 애플리케이션 수준 인증이 없습니다. loopback 이외 주소에
바인드하는 배포에서는 Windows 방화벽으로 접근 가능한 출발지와 포트를
제한해야 합니다.

## 의존성과 release 최적화

주요 런타임 의존성은 Axum, Tokio, Reqwest, Serde, Thiserror,
`windows-sys`입니다. Reqwest는 native TLS 대신 `rustls-tls`를 사용하고,
Axum과 Tokio는 필요한 feature만 활성화합니다.

release profile은 작은 실행 파일을 위해 다음 설정을 사용합니다.

- `opt-level = "z"`
- LTO 활성화
- codegen unit 1개
- panic abort
- symbol strip

`Cargo.lock`은 재현 가능한 dependency resolve를 위해 커밋합니다.

## 빌드와 검증

모든 Cargo 명령은 지정된 절대 경로를 사용하며 프로젝트 루트에서 실행합니다.

```powershell
& 'C:\code\path\cargo\bin\cargo.exe' fmt --check
& 'C:\code\path\cargo\bin\cargo.exe' clippy --all-targets -- -D warnings
& 'C:\code\path\cargo\bin\cargo.exe' test
& 'C:\code\path\cargo\bin\cargo.exe' build --release
```

release 산출물은 `target\release\background_codex_quota.exe`입니다.

테스트는 최소한 다음 계약을 보호해야 합니다.

- weekly, monthly, extra 파싱
- 누락, 잘못된 타입, 범위 밖 사용률
- snake_case와 camelCase
- round-half-even과 clamp 경계
- Auth 경로와 지원 필드 우선순위
- 손상되거나 토큰이 없는 Auth JSON
- upstream 인증 헤더와 HTTP 상태 매핑
- `/health`, `/quota/usage`, `/control/shutdown`
- named mutex 중복 실행과 graceful shutdown

## 배포 절차

1. format, Clippy, test, release build를 모두 통과시킵니다.
2. 기존 프로세스에 `POST /control/shutdown`을 호출합니다.
3. 프로세스와 포트가 해제되었는지 확인합니다.
4. `target\release\background_codex_quota.exe`를 루트의
   `Background_Codex_Quota.exe`로 복사합니다.
5. 배포 실행 파일을 시작합니다.
6. `GET /health`가 `200`인지 확인합니다.
7. fixture 또는 실제 Auth 환경에서 `GET /quota/usage`의 상태와 고정 JSON
   구조를 확인합니다.
8. 종료 API 호출 후 프로세스와 포트가 다시 정상적으로 해제되는지 확인합니다.

PowerShell smoke test 예시:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/health'
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/quota/usage'
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8787/control/shutdown'
```

## API 변경 시 확인 목록

- 성공 JSON 필드 이름과 `null` 가능성을 의도적으로 변경했는가
- 값의 의미가 사용률인지 남은 비율인지 테스트로 고정했는가
- 일반 사용자용 `README.md`와 개발자 가이드를 함께 갱신했는가
- 외부 GUI와 Stream Deck 등 기존 소비자의 endpoint를 확인했는가
- 종료 endpoint의 네트워크 노출 범위를 검토했는가
- 인증 정보가 오류 응답이나 로그에 들어가지 않는가
