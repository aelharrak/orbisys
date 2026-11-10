# OrbiSys

**하나의 앱에서 Linux 유지 관리와 시스템 관리를 수행하세요.** OrbiSys는 동일한 Rust 코어를 사용하는 그래픽 인터페이스와 CLI를 제공합니다.

[![버전](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![MIT 라이선스](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="OrbiSys 로고" width="112"></p>

**언어:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · 한국어 · [中文](README.zh.md) · [العربية](README.ar.md)

## 기능

OrbiSys는 시스템 점검과 유지 관리 도구를 한곳에 모았습니다.

- **시스템 및 모니터링:** 시스템 상태, 프로세스, 서비스, 로그, 시작 항목, 예약 작업.
- **소프트웨어:** 기본 패키지, Snap, Flatpak 및 수동 설치 앱 목록, 업데이트, 저장소, 런타임.
- **저장소 및 백업:** 디스크, 마운트, 정리, 아카이브, Btrfs, 스냅샷, 백업.
- **네트워크 및 하드웨어:** 로컬 네트워크, 트래픽, Wi-Fi, VPN, Bluetooth, USB, 전원, 센서, 펌웨어.
- **보안:** 감사, 방화벽, Secure Boot, AppArmor, 인증서, SSH/GPG 키, 무결성 검사.
- **내보내기:** 인벤토리는 TXT/CSV/JSON, 시스템 보고서는 TXT/CSV/JSON/PDF 형식으로 저장합니다.

일부 기능은 배포판에 설치된 시스템 도구에 따라 달라집니다. 인터페이스 테마와 언어를 설정할 수 있습니다.

## 설치

미리 빌드한 패키지는 [`dist/`](dist/)에 있으며 파일명에 지원 아키텍처가 표시됩니다. 저장소 루트에서 배포판에 맞는 패키지를 설치하세요.

| 배포판 | 설치 명령 |
| --- | --- |
| Fedora, RHEL 및 호환 배포판 | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian, Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux 및 파생 배포판 | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

포함된 Alpine 패키지는 설정된 저장소에서 서명한 파일이 아닌 로컬 파일이므로 `--allow-untrusted`가 필요합니다. 다른 패키지 관리자는 로컬 파일을 설치하고 선언된 의존성을 해결합니다. 관리자 권한 작업에는 Polkit이 필요합니다. Debian 패키지는 이를 권장하고 다른 형식은 의존성으로 선언합니다.

설치 후 터미널에서 `orbisys`를 실행하거나 애플리케이션 메뉴에서 **OrbiSys**를 여세요.

### 소스에서 빌드

요구 사항: Rust 및 Cargo 1.80 이상. 프로젝트 루트에서 실행합니다.

```sh
cargo build --release
./target/release/orbisys
```

인자 없이 실행하면 그래픽 인터페이스가 열립니다. CLI는 `./target/release/orbisys help`로 실행하세요. 개발 중에는 예를 들어 `cargo run -- help`를 사용할 수 있습니다.

## 빠른 시작

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # 삭제 없이 정리 대상 확인
orbisys report ./reports
orbisys panel                 # 패널 목록 보기
orbisys panel monitor
```

## CLI 명령

| 명령 | 설명 |
| --- | --- |
| `help`, `--help`, `-h` | 도움말을 표시합니다. |
| `version`, `--version` | 버전을 표시합니다. |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | 소프트웨어 목록을 표시하고 요청한 형식으로 내보냅니다. |
| `updates` | 감지된 패키지 관리자를 통해 업데이트를 확인합니다. |
| `clean` | 삭제 없이 정리 대상을 표시합니다. |
| `clean --apply` | 지원되는 사용자 영역을 정리합니다. root 권한이 필요한 시스템 대상은 CLI에서 건너뜁니다. |
| `report [DIRECTORY]` | TXT/CSV/JSON/PDF 보고서를 지정한 디렉터리 또는 기본 Documents 디렉터리에 저장합니다. |
| `panel` / `panel NAME` | 패널 목록 표시 / 읽기 전용으로 패널 표시. |

`orbisys panel`에서 사용할 수 있는 이름을 확인할 수 있습니다. 예: `monitor`, `health`, `mounts`, `lan`, `certs`.

## 보안 및 권한

조회 화면과 진단 명령은 시스템을 변경하지 않습니다. 관리자 권한이 필요한 작업은 Polkit(`pkexec`)으로 위임되며 인증이 필요합니다. CLI의 `clean --apply`는 사용자 영역만 처리합니다.

OrbiSys는 프로그램을 인자별로 실행하고 패키지 이름, 유닛 이름, 경로를 사용 전에 검증합니다. 사용 가능한 시스템 도구는 배포판마다 다를 수 있습니다.

## 패키지 빌드

[`packaging/`](packaging/)의 스크립트로 release 바이너리에서 DEB, RPM, Arch, Alpine 패키지를 만들 수 있습니다.

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# 모든 형식 빌드 시도
./packaging/build-all.sh
```

필요한 도구: DEB는 `dpkg-deb`, RPM은 `rpmbuild`, Arch는 `tar`와 `zstd`, Alpine은 `apk-tools` 3 또는 Docker입니다. 패키지는 `dist/`에 생성됩니다. 일부 Debian/Ubuntu 시스템에서는 RPM 스크립트가 필요한 도구를 로컬로 가져올 수 있습니다.

## 언어

인터페이스는 프랑스어, 영어, 독일어, 스페인어, 포르투갈어, 이탈리아어, 네덜란드어, 일본어, 한국어, 중국어, 아랍어를 지원합니다. 프랑스어·영어·스페인어 번역은 완전하며 다른 언어는 일부 번역이 빠져 있을 수 있습니다. 번역되지 않은 문구는 영어로 표시됩니다.

## 프로젝트 구조

- `src/main.rs`: 진입점 및 GUI/CLI 선택.
- `src/lib.rs`: 공용 코어와 시스템 모듈.
- `src/cli.rs`: CLI 명령.
- `src/gui.rs`, `src/gui/`: 그래픽 인터페이스.
- `src/p_*.rs`, `src/panels.rs`: 전문 패널.
- `src/lang/`: 앱 번역 파일.
- `packaging/`: Linux 패키지 스크립트와 메타데이터.
- `assets/`: 그래픽 리소스, `dist/`: 포함된 사전 빌드 패키지.

## 라이선스

프로젝트는 [`Cargo.toml`](Cargo.toml)에 MIT 라이선스를 선언합니다. 포함된 Lucide 아이콘은 [`assets/icons/LICENSE`](assets/icons/LICENSE)에 별도 라이선스가 있습니다.
