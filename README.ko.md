# 자체 이벤트 수집기

[한국어](README.ko.md) | [English](README.md)

## 프로젝트 개요

어플리케이션에서 전송되는 이벤트를 빠르고 정확하게 수집할 수 있도록 설계된 이벤트 수집 프로젝트입니다

채널과 여러개의 병렬 쓰레드, 스케쥴러 등을 동시에 활용해 효율적으로 이벤트가 관리되고 있습니다

또한 오류가 발생되거나 메모리 한계를 고려해 임시로 데이터를 보관하는 기능도 구현되어있습니다

### 주요 기능

- **이벤트 수집**: HTTP POST 요청으로 이벤트 데이터 수집
- **오픈서치 연동**: 실시간으로 이벤트 데이터를 오픈서치에 대량 등록
- **데이터베이스 백업**: 채널에 많은 데이터가 누적되어있거나 오픈서치 저장 실패 시 데이터베이스에 펜딩
- **인증 미들웨어**: Bearer 토큰을 사용해 API 보안을 유지
- **운영 중 상태 모니터링**: `/ping` 엔드포인트로 서버 상태 확인
- **비동기 작업 처리**: 오픈서치 및 펜딩 데이터베이스와의 비동기 통신
- **적절한 리소스 설정**: 채널은 메모리에 저장되는 영역으로 10만개로 설정했으며 오픈서치 대량 등록 쓰레드는 10개로 지정

## 설치 및 실행

### 1. 러스트 설치

이 프로젝트는 Rust를 이용해서 개발되었습니다. 시스템에 Rust 및 Cargo가 설치되어 있어야 합니다.

### 2. 프로젝트 클론

``` bash
git clone https://github.com/lee-lou2/rust-event-collector
cd https://github.com/lee-lou2/rust-event-collector
```

### 3. 환경 변수 설정

프로젝트 실행 전, `.env` 파일에 다음 변수를 추가하세요:

``` dotenv
OPEN_SEARCH_DNS=http://localhost:9200
SERVER_PORT=3000
JWT_SECRET=
SERVER_ENVIRONMENT=local
DATABASE_URL=sqlite://sqlite3.db
```

### 4. 서버 실행

#### cargo 를 이용한 실행

``` bash
cargo run
```

#### docker 를 이용한 실행

```bash
sh deploy.sh
```

서버가 다음 주소에서 실행됩니다: `http://0.0.0.0:3000` (환경 변수에 설정된 포트에 따라 다를 수 있음)

## 시스템 구조

1. **핵심 구성 요소**:
    - **Axum**: HTTP 서버 프레임워크
    - **OpenSearch**: 데이터 저장소
    - **SQLite**: 실패한 이벤트 데이터 임시 저장소
    - **Tokio**: 비동기 작업 실행기

2. **작업 흐름**:
    - 클라이언트가 `/events` API를 호출하고 이벤트 데이터를 전송합니다.
    - **성공**:
        - 이벤트는 채널에 전달되도록 시도합니다.
            - 기본적으로 1000개의 채널이 준비되어있습니다.
            - 모든 채널이 꽉 채워져있다면 데이터베이스에 펜딩 데이터를 저장합니다.
            - 채워지지 않았다면 채널에 전달됩니다.
        - 채널에 전달된 데이터는 1000개씩 묶어서 오픈서치에 대량 등록합니다.
            - 1000개까지 묶이지 않더라도 10초 단위로 채널에 있는 이벤트를 오픈서치로 대량 등록합니다.
    - **실패**:
        - 채널이 꽉 채워져있거나 오픈서치 등록간 오류가 발생한다면 데이터베이스에 펜딩 데이터를 저장합니다.
            - 이렇게 저장된 데이터는 1분 간격으로 조회되어 채널로 다시 전달을 시도합니다.

    - 스케줄러는 펜딩된 데이터를 주기적으로 채널로 전송 시도합니다.

## Flowchart

시스템의 동작 흐름은 아래의 이미지를 참조하세요

![flowchart.png](docs/flowchart.png)

## API 문서

### **인증**

모든 엔드포인트는 `/ping`을 제외하고 헤더에 Bearer 토큰이 필요합니다.

잘못된 토큰이 포함되거나 누락된 경우, HTTP 401 응답이 반환됩니다.

### **POST /events**

- **설명**: 이벤트를 빠르고 안전하게 저장합니다.
- **HTTP 메서드**: POST
- **헤더**:
    - `Authorization: Bearer <API_KEY>` (필수): API 접근을 위한 인증 토큰
    - `device-uuid` (선택): 이벤트가 발생한 디바이스의 고유 식별자
    - `app-version` (선택): 앱 버전 정보
    - `os-version` (선택): 운영체제 버전 정보
- **요청 본문**: JSON
    - 필드:
        - `log_id` (필수): 고유한 로그 식별자
        - `page` (필수): 이벤트가 발생한 페이지
        - `event` (필수): 이벤트 이름
        - `label` (선택): 이벤트 레이블
        - `target` (선택): 이벤트 대상
        - `section` (선택): 이벤트가 발생한 섹션
        - `param` (선택): 추가 파라미터(JSON)
- **요청 예시**:

```bash
curl -X POST http://0.0.0.0:3000/events \
-H "Authorization: Bearer <API_KEY>" \
-H "Content-Type: application/json" \
-H "device-uuid: <DEVICE_UUID>" \
-H "app-version: <APP_VERSION>" \
-H "os-version: <OS_VERSION>" \
-d '{
  "page": "/home",
  "event": "click",
  "label": "button_click",
  "target": "#button_id",
  "section": "header",
  "param": {
    "key1": "value1"
  }
}'
```

- **응답**:
    - HTTP 200: 이벤트 등록 요청 성공

### **GET /ping**

- **설명**: 시스템 상태를 확인합니다.
- **응답**:
    - HTTP 200: "pong" 문자열 반환

## 프로젝트 로드맵

- [ ] 이벤트 데이터 지연 시간 최적화
- [ ] 오픈서치 및 데이터베이스 연결 오류에 대한 세부적인 처리
- [ ] 이벤트 백업 데이터 주기적 정리 기능 추가
