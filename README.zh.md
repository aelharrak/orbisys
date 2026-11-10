# OrbiSys

**在一个应用中维护和管理 Linux 系统。** OrbiSys 提供图形界面和命令行工具（CLI），二者共用同一 Rust 核心。

[![版本](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![MIT 许可证](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="OrbiSys 标志" width="112"></p>

**语言：** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · 中文 · [العربية](README.ar.md)

## 功能

OrbiSys 汇集了系统检查和维护工具：

- **系统与监控：** 系统状态、进程、服务、日志、启动项和计划任务。
- **软件：** 原生软件包、Snap、Flatpak 和手动安装应用的清单；更新、软件源和运行时。
- **存储与备份：** 磁盘、挂载、清理、归档、Btrfs、快照和备份。
- **网络与硬件：** 局域网、流量、Wi-Fi、VPN、蓝牙、USB、电源、传感器和固件。
- **安全：** 审计、防火墙、Secure Boot、AppArmor、证书、SSH/GPG 密钥和完整性检查。
- **导出：** 清单支持 TXT、CSV、JSON；系统报告支持 TXT、CSV、JSON、PDF。

部分功能取决于发行版中已安装的系统工具。界面主题和语言均可配置。

## 安装

预编译软件包位于 [`dist/`](dist/)，文件名标明适用架构。请在仓库根目录中选择适合发行版的软件包并执行对应命令。

| 发行版 | 安装命令 |
| --- | --- |
| Fedora、RHEL 及兼容系统 | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian、Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux 及衍生版 | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

附带的 Alpine 软件包是本地文件，未由已配置的软件源签名，因此需要 `--allow-untrusted`。其他软件包管理器会安装本地文件并解决已声明的依赖。特权操作需要 Polkit：Debian 软件包将其列为推荐依赖，其他格式将其列为依赖。

安装后，可在终端运行 `orbisys`，或从应用程序菜单启动 **OrbiSys**。

## 卸载

使用发行版的软件包管理器卸载 OrbiSys：

| 发行版 | 命令 |
| --- | --- |
| Fedora、RHEL | `sudo dnf remove orbisys` |
| openSUSE | `sudo zypper remove orbisys` |
| Debian、Ubuntu | `sudo apt remove orbisys` |
| Arch Linux | `sudo pacman -R orbisys` |
| Alpine Linux | `sudo apk del orbisys` |

如果通过 PPA 安装，还可以使用 `sudo add-apt-repository --remove ppa:amine-elharrak/ppa` 单独移除该软件源。如果从源代码构建，请先关闭应用，再删除克隆的仓库目录。

### 从源代码构建

要求：Rust 和 Cargo 1.80 或更高版本。在项目根目录运行：

```sh
cargo build --release
./target/release/orbisys
```

不带参数时会启动图形界面。运行 `./target/release/orbisys help` 使用 CLI。开发时也可运行 `cargo run -- help`。

## 快速开始

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # 仅检查，不删除
orbisys report ./reports
orbisys panel                 # 列出面板
orbisys panel monitor
```

## CLI 命令

| 命令 | 说明 |
| --- | --- |
| `help`、`--help`、`-h` | 显示帮助。 |
| `version`、`--version` | 显示版本。 |
| `inventory [--txt FILE] [--csv FILE] [--json FILE]` | 显示软件清单并写出指定格式的导出文件。 |
| `updates` | 使用检测到的软件包管理器检查更新。 |
| `clean` | 显示清理目标和大小，不执行删除。 |
| `clean --apply` | 清理支持的用户级目标；CLI 会跳过需要 root 权限的系统目标。 |
| `report [DIRECTORY]` | 将 TXT、CSV、JSON、PDF 报告写入指定目录，或默认的 Documents 目录。 |
| `panel` / `panel NAME` | 列出面板 / 以只读方式显示面板信息。 |

运行 `orbisys panel` 查看可用名称，例如 `monitor`、`health`、`mounts`、`lan` 和 `certs`。

## 安全与权限

信息查看页面和诊断命令不会更改系统。需要提升权限的操作通过 Polkit（`pkexec`）执行，并要求授权。CLI 的 `clean --apply` 只处理用户级目标。

OrbiSys 使用独立参数启动程序，并在使用前验证软件包名、单元名和路径。各发行版可用的系统工具可能不同。

## 构建软件包

[`packaging/`](packaging/) 中的脚本可从 release 二进制构建 DEB、RPM、Arch 和 Alpine 软件包：

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# 尝试构建所有格式
./packaging/build-all.sh
```

所需工具：DEB 使用 `dpkg-deb`，RPM 使用 `rpmbuild`，Arch 使用 `tar` 和 `zstd`，Alpine 使用 `apk-tools` 3 或 Docker。软件包生成到 `dist/`。在某些 Debian/Ubuntu 系统上，RPM 脚本可以在本地获取所需工具。

## 语言

界面支持法语、英语、德语、西班牙语、葡萄牙语、意大利语、荷兰语、日语、韩语、中文和阿拉伯语。法语、英语和西班牙语翻译完整；其他语言可能尚未完全翻译，缺失文本会回退到英语。

## 项目结构

- `src/main.rs`：入口以及 GUI/CLI 选择。
- `src/lib.rs`：共享核心与系统模块。
- `src/cli.rs`：CLI 命令。
- `src/gui.rs`、`src/gui/`：图形界面。
- `src/p_*.rs`、`src/panels.rs`：专用面板。
- `src/lang/`：应用翻译文件。
- `packaging/`：Linux 软件包脚本和元数据。
- `assets/`：图形资源；`dist/`：随项目提供的预编译软件包。

## 许可证

项目在 [`Cargo.toml`](Cargo.toml) 中声明使用 MIT 许可证。随附的 Lucide 图标采用单独许可证，见 [`assets/icons/LICENSE`](assets/icons/LICENSE)。
