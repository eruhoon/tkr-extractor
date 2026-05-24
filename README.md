# TKR Extractor

RPG Maker VX Ace 아카이브 복호화 및 로컬 LLM(Ollama) 연동을 통한 게임 텍스트/이미지 기계 번역 지원 데스크톱 유틸리티입니다. Rust + Tauri 2.0 및 Svelte 5 기반으로 설계되어 가볍고 안전하게 작동합니다.

---

## 📦 주요 기능

### 1. RGSS3A 디컴파일러 (Archive Extractor)
* RPG Maker VX Ace 아카이브 파일인 `Game.rgss3a`, `Game.rgssad`, `Game.rgss2a` 복호화 및 내장 파일 구조를 그대로 추출합니다.
* 실시간 복호화 작업 로그 및 프로그레스 정보를 제공합니다.
* 디컴파일 성공 시 복호화된 폴더를 탐색기/파인더로 즉시 열 수 있는 숏컷 기능을 지원합니다.

### 2. 스크립트 번역기 (Scripts Translator)
* `Scripts.rvdata2` 및 `Scripts.rvdata` 파일을 파싱하여 내부 코드로부터 일본어 문자열(대사, 메뉴 등)을 자동으로 추출합니다.
* 로컬 Ollama API (`http://127.0.0.1:11434`)와 연동하여 한국어로의 1:1 및 일괄 번역 기능을 제공합니다.
* Svelte 5 룬즈(`$state()`)를 적용한 실시간 반응형 데이터 동기화 및 3단 정렬 레이아웃을 통해 번역 편의성을 높였습니다.
* Ollama 통신 전과정을 담은 실시간 디버그 로그 패널 및 에러 메시지 클릭 복사 기능을 내장하고 있습니다.

### 3. 이미지 번역기 (Image Translator)
* 특정 이미지 폴더나 개별 이미지 파일을 드래그 앤 드롭하여 로컬 비전 모델(Ollama)에 전달합니다.
* 이미지 내부의 일본어 텍스트 위치를 탐지하고 한국어로 번역한 결과를 원본 이미지 위에 합성하여 저장합니다. (검은색 반투명 가림막 및 흰색 글씨 + 검은색 테두리 아웃라인 물리 합성 렌더링)
* 누적 이미지 처리 개수를 기록하며, 전 과정 100% 로컬 무료 구동됩니다.

---

## 🛠️ 기술 스택

* **Frontend**: SvelteKit, TypeScript, Vanilla SCSS, Svelte 5 Runes
* **Backend**: Rust, Tauri 2.0 (`tauri-plugin-dialog`, `tauri-plugin-http`, `tauri-plugin-opener` 등)
* **Encryption Library**: `rpgmad-lib = "5.0.0"` (rpgmaker 복호화)
* **Image Processing**: `image = "0.24"`, `imageproc = "0.23"`, `ab_glyph = "0.2"` (물리 이미지 합성)
* **LLM Engine**: Ollama (Local AI)

---

## 🚀 시작 방법 (Local Development)

### 1. 요구 사항
* [Node.js](https://nodejs.org/) (패키지 매니저로 `pnpm` 권장)
* [Rust](https://www.rust-lang.org/) 컴파일러 및 빌드 도구
* [Ollama](https://ollama.com/) 로컬 인스턴스

### 2. 로컬 개발 서버 구동
1. 패키지 설치:
   ```bash
   pnpm install
   ```
2. 번역에 사용할 Ollama 로컬 모델 준비 (예: `gemma4:e4b` 또는 `gemma2:2b` 등):
   ```bash
   ollama pull gemma4:e4b
   ```
3. Tauri 개발 서버 구동:
   ```bash
   pnpm tauri dev
   ```

### 3. 배포용 빌드
```bash
pnpm tauri build
```

---

## 💡 문제 해결 (Troubleshooting)

### Ollama 통신 실패: 404 Not Found 가 발생해요
* Ollama에 API 요청 시 지정한 모델명이 로컬 컴퓨터에 다운로드되어 있지 않으면 해당 에러가 발생합니다.
* 터미널에 `ollama list` 명령을 실행해 지정하려는 모델명이 목록에 있는지 체크해 주시고, 없는 경우 `ollama pull [모델명]`을 수행해 다운로드해 주세요.
* 스크립트/이미지 번역기 우측 상단 인풋 창에 직접 다운로드받으신 모델명을 정확히 입력하여 변경하실 수 있습니다.

### 드래그 앤 드롭 파일 로드가 안 돼요
* 활성화된 각 탭 종류에 따라 수락하는 파일 형식이 격리되어 있습니다.
  * **디컴파일러**: `.rgss3a`, `.rgssad`, `.rgss2a` 아카이브 파일만 허용
  * **스크립트 번역기**: `.rvdata`, `.rvdata2` 파일만 허용
  * **이미지 번역기**: 이미지 파일(`.png`, `.jpg`, `.jpeg`, `.bmp`) 또는 디렉토리(폴더)만 허용
* 올바른 확장자의 파일을 올바른 탭에 드롭하셨는지 다시 한번 체크해 주세요.
