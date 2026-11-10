# OrbiSys

**Linux の保守とシステム管理をひとつのアプリで。** OrbiSys は、同じ Rust コアを使う GUI と CLI を提供します。

[![バージョン](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![MIT ライセンス](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="OrbiSys ロゴ" width="112"></p>

**言語:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · 日本語 · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## 主な機能

OrbiSys はシステムの確認と保守のためのツールをまとめています。

- **システムと監視:** システム状態、プロセス、サービス、ログ、起動項目、スケジュール済みタスク。
- **ソフトウェア:** ネイティブパッケージ、Snap、Flatpak、手動インストールアプリの一覧、更新、リポジトリ、ランタイム。
- **ストレージとバックアップ:** ディスク、マウント、クリーンアップ、アーカイブ、Btrfs、スナップショット、バックアップ。
- **ネットワークとハードウェア:** ローカルネットワーク、通信量、Wi-Fi、VPN、Bluetooth、USB、電源、センサー、ファームウェア。
- **セキュリティ:** 監査、ファイアウォール、Secure Boot、AppArmor、証明書、SSH/GPG キー、整合性チェック。
- **エクスポート:** インベントリは TXT/CSV/JSON、システムレポートは TXT/CSV/JSON/PDF。

一部の機能には、ディストリビューションに応じたシステムツールが必要です。テーマと言語は設定できます。

## インストール

ビルド済みパッケージは [`dist/`](dist/) にあります。ファイル名に記載されたアーキテクチャを確認し、リポジトリのルートで該当するコマンドを実行してください。

| ディストリビューション | インストールコマンド |
| --- | --- |
| Fedora、RHEL および互換環境 | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian、Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux と派生版 | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

同梱の Alpine パッケージは、設定済みリポジトリによる署名のないローカルファイルのため、`--allow-untrusted` が必要です。ほかのパッケージマネージャーはローカルファイルをインストールし、宣言された依存関係を解決します。特権操作には Polkit が必要です。Debian パッケージでは推奨依存、ほかの形式では依存関係として宣言されています。

インストール後、端末で `orbisys` を実行するか、アプリケーションメニューから **OrbiSys** を起動してください。

## アンインストール

ディストリビューションのパッケージマネージャーで OrbiSys を削除します。

| ディストリビューション | コマンド |
| --- | --- |
| Fedora / RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian / Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

PPA からインストールした場合は、`sudo add-apt-repository --remove ppa:amine-elharrak/ppa` で PPA も削除できます。ソースからビルドした場合は、アプリを終了してクローンしたリポジトリのフォルダーを削除してください。

### ソースからビルド

必要条件: Rust と Cargo 1.80 以降。プロジェクトのルートで実行します。

```sh
cargo build --release
./target/release/orbisys
```

引数なしで GUI が起動します。CLI は `./target/release/orbisys help` で利用できます。開発時は `cargo run -- help` などを実行します。

## クイックスタート

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # 削除せずに対象を確認
orbisys report ./reports
orbisys panel                 # パネル一覧
orbisys panel monitor
```

## CLI コマンド

| コマンド | 説明 |
| --- | --- |
| `help`, `--help`, `-h` | ヘルプを表示します。 |
| `version`, `--version` | バージョンを表示します。 |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | ソフトウェア一覧を表示し、指定された形式で出力します。 |
| `updates` | 検出されたパッケージマネージャーで更新を確認します。 |
| `clean` | 削除せずにクリーンアップ対象を表示します。 |
| `clean --apply` | 対応するユーザー領域を整理します。root 権限が必要なシステム対象は CLI ではスキップされます。 |
| `report [DIRECTORY]` | TXT/CSV/JSON/PDF レポートを指定先、または既定の Documents フォルダーに出力します。 |
| `panel` / `panel NAME` | パネル一覧を表示 / 読み取り専用でパネルを表示します。 |

`orbisys panel` で有効な名前を確認できます。例: `monitor`、`health`、`mounts`、`lan`、`certs`。

## セキュリティと権限

参照画面と診断コマンドはシステムを変更しません。特権が必要な操作は Polkit (`pkexec`) に委譲され、認証が必要です。CLI の `clean --apply` はユーザー領域のみを処理します。

OrbiSys はプログラムを引数ごとに起動し、パッケージ名、ユニット名、パスを使用前に検証します。利用できるシステムツールはディストリビューションにより異なります。

## パッケージのビルド

[`packaging/`](packaging/) のスクリプトで、release バイナリから DEB、RPM、Arch、Alpine パッケージを作成できます。

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# 全形式を試す
./packaging/build-all.sh
```

必要なツール: DEB は `dpkg-deb`、RPM は `rpmbuild`、Arch は `tar` と `zstd`、Alpine は `apk-tools` 3 または Docker です。パッケージは `dist/` に作成されます。一部の Debian/Ubuntu 環境では RPM ビルドスクリプトがツールをローカルに取得します。

## 言語

インターフェースはフランス語、英語、ドイツ語、スペイン語、ポルトガル語、イタリア語、オランダ語、日本語、韓国語、中国語、アラビア語に対応しています。フランス語・英語・スペイン語は翻訳が完了しており、ほかの言語は一部未翻訳の場合があります。未翻訳部分は英語で表示されます。

## プロジェクト構成

- `src/main.rs`: エントリーポイント、GUI/CLI の選択。
- `src/lib.rs`: 共通コアとシステムモジュール。
- `src/cli.rs`: CLI コマンド。
- `src/gui.rs`, `src/gui/`: GUI。
- `src/p_*.rs`, `src/panels.rs`: 専用パネル。
- `src/lang/`: アプリの翻訳ファイル。
- `packaging/`: Linux パッケージ用スクリプトとメタデータ。
- `assets/`: グラフィック素材、`dist/`: ビルド済みパッケージ。

## ライセンス

プロジェクトのライセンスは [`Cargo.toml`](Cargo.toml) に記載された MIT です。同梱の Lucide アイコンには [`assets/icons/LICENSE`](assets/icons/LICENSE) に別ライセンスがあります。
