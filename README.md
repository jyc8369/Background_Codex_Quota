# Background_Codex_Quota

`Background_Codex_Quota`는 로컬 Codex 인증 정보를 사용해 현재 쿼터
사용률을 조회하고, 다른 GUI 프로그램이나 자동화 도구가 사용할 수 있도록
HTTP API로 제공하는 Windows x64용 초경량 백그라운드 프로그램입니다.

개발 및 배포 정보는 [개발자 가이드](docs/DEVELOPER_GUIDE.md)를 참고하십시오.

## 주요 특징

- 시스템 트레이와 자체 GUI를 사용하지 않습니다.
- 기본 주소는 `http://127.0.0.1:8787`입니다.
- 쿼터 값은 **남은 비율이 아니라 사용한 비율**입니다.
- 상태 확인, 쿼터 조회, 정상 종료를 HTTP API로 제어합니다.

## 실행 전 준비

다음 항목이 필요합니다.

- `Background_Codex_Quota.exe`
- Codex 로그인으로 생성된 `auth.json`
- 기본 포트 `8787`을 사용할 수 있는 환경

인증 파일은 다음 순서로 탐색합니다. 먼저 발견된 경로만 사용합니다.

1. `CODEX_AUTH_JSON`에 지정한 파일
2. `CODEX_HOME\auth.json`
3. `%USERPROFILE%\.codex\auth.json`

인증 토큰은 외부에 공유하지 마십시오. 프로그램은 토큰과 계정 ID를 로그에
기록하지 않습니다.

## 실행하기

파일 탐색기에서 `Background_Codex_Quota.exe`를 실행하거나 PowerShell에서
다음과 같이 실행합니다.

```powershell
& 'C:\프로그램경로\Background_Codex_Quota.exe'
```

아무 창이나 트레이 아이콘이 나타나지 않는 것이 정상입니다. 같은 Windows
사용자 세션에서 두 번째 인스턴스를 실행하면 새 인스턴스는 즉시 종료됩니다.

실행 상태를 확인합니다.

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/health'
```

정상이면 다음 응답을 반환합니다.

```json
{
  "status": "ok"
}
```

## 주소와 포트 변경

기본 주소는 `127.0.0.1:8787`입니다. 실행 전에 환경변수를 지정하면 변경할
수 있습니다.

```powershell
$env:CODEX_QUOTA_HOST = '127.0.0.1'
$env:CODEX_QUOTA_PORT = '8787'
& 'C:\프로그램경로\Background_Codex_Quota.exe'
```

`CODEX_QUOTA_HOST`에는 IP 주소를, `CODEX_QUOTA_PORT`에는 `1`부터
`65535` 사이의 포트를 지정합니다. 환경변수를 바꿨다면 실행 중인 프로그램을
종료한 뒤 다시 시작해야 합니다.

특정 인증 파일을 사용하려면 실행 전에 다음과 같이 지정합니다.

```powershell
$env:CODEX_AUTH_JSON = 'C:\Users\사용자명\.codex\auth.json'
& 'C:\프로그램경로\Background_Codex_Quota.exe'
```

## API 사용법

### 상태 확인

```http
GET /health
```

성공 상태는 `200 OK`이며 응답은 다음과 같습니다.

```json
{
  "status": "ok"
}
```

### 쿼터 사용률 조회

```http
GET /quota/usage
```

PowerShell 예시:

```powershell
Invoke-RestMethod -Uri 'http://127.0.0.1:8787/quota/usage'
```

성공 응답:

```json
{
  "weekly": 28,
  "monthly": null,
  "extra": 17
}
```

각 숫자는 이미 사용한 비율이며 범위는 `0`부터 `100`까지입니다.

- `weekly`: 주 단위 기본 쿼터 사용률
- `monthly`: 월 단위 기본 쿼터 사용률
- `extra`: 첫 번째 추가 한도 사용률
- `null`: 해당 종류의 한도가 현재 응답에 없음을 의미

기본 쿼터는 `weekly` 또는 `monthly` 중 해당하는 한 곳에만 채워질 수 있습니다.
조회할 때마다 upstream 서버에서 최신 값을 다시 가져오므로 별도의 새로고침
API는 없습니다.

### 정상 종료

```http
POST /control/shutdown
```

PowerShell 예시:

```powershell
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8787/control/shutdown'
```

요청을 받으면 `202 Accepted`와 다음 응답을 반환한 뒤 HTTP 포트와 프로세스를
정상적으로 해제합니다.

```json
{
  "status": "shutting_down"
}
```

## 오류 응답

| HTTP 상태 | 의미 | 확인할 내용 |
| --- | --- | --- |
| `401` | 인증 파일 또는 토큰 문제 | `auth.json` 경로, 로그인 상태, 토큰 필드 확인 |
| `502` | upstream 서버가 오류 상태를 반환 | 잠시 후 재시도하거나 Codex 서비스 상태 확인 |
| `500` | 네트워크, JSON 해석 또는 내부 처리 오류 | 네트워크 연결과 진단 출력 확인 |

오류 응답에는 `detail` 필드가 포함됩니다. 인증 토큰과 계정 ID는 응답에
포함되지 않습니다.

## 보안과 원격 제어

기본값 `127.0.0.1`은 같은 PC에서만 접근할 수 있습니다. 다른 기기에서
접근하게 하려면 `CODEX_QUOTA_HOST`를 해당 PC의 IP 또는 적절한 바인드 주소로
변경하고 Windows 방화벽 규칙을 설정해야 합니다.

종료 API에는 별도 인증 기능이 없습니다. 원격 접근을 허용하는 경우 신뢰할 수
있는 IP와 네트워크만 접근하도록 방화벽에서 제한하십시오.

## 문제 해결

### 실행했지만 아무것도 보이지 않음

정상 동작입니다. 이 프로그램은 트레이와 자체 창을 사용하지 않습니다.
`GET /health`로 실행 상태를 확인하십시오.

### `/health`에 연결할 수 없음

- 다른 프로세스가 포트 `8787`을 사용 중인지 확인합니다.
- 변경한 `CODEX_QUOTA_HOST`와 `CODEX_QUOTA_PORT`를 확인합니다.
- 이미 실행 중이던 인스턴스가 다른 주소에 바인드되어 있지 않은지 확인합니다.
- Windows 방화벽의 인바운드 규칙을 확인합니다.

### `/quota/usage`가 `401`을 반환함

- Codex에 다시 로그인해 `auth.json`이 생성되었는지 확인합니다.
- `CODEX_AUTH_JSON`과 `CODEX_HOME`에 오래된 경로가 지정되어 있지 않은지
  확인합니다.
- 지정한 파일을 현재 Windows 사용자가 읽을 수 있는지 확인합니다.

### 로그 파일을 찾을 수 없음

로그 파일을 생성하지 않는 것이 정상입니다. 진단 메시지는 `stderr`로만
출력되며, 부모 콘솔에 연결된 개발·진단 실행에서만 확인할 수 있습니다.

## 업데이트

실행 파일을 교체하기 전에 종료 API를 호출하고 프로세스가 끝날 때까지
기다립니다. 그 후 새 `Background_Codex_Quota.exe`로 교체하고 다시
실행한 다음 `/health`와 `/quota/usage`를 확인합니다.
