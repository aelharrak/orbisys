# OrbiSys

**Mantenimiento y administración de Linux en una sola aplicación.** OrbiSys reúne una interfaz gráfica y una CLI basadas en el mismo núcleo Rust.

[![Versión](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![Licencia MIT](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="Logotipo de OrbiSys" width="112"></p>

**Idiomas:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · Español · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Funciones

OrbiSys reúne herramientas de consulta y mantenimiento del sistema:

- **Sistema y supervisión:** estado del equipo, procesos, servicios, registros, inicio y tareas programadas.
- **Software:** inventario de paquetes nativos, Snap, Flatpak y aplicaciones instaladas manualmente; actualizaciones, repositorios y entornos de ejecución.
- **Almacenamiento y copias de seguridad:** discos, montajes, limpieza, archivos, Btrfs, instantáneas y copias.
- **Red y hardware:** red local, tráfico, Wi-Fi, VPN, Bluetooth, USB, energía, sensores y firmware.
- **Seguridad:** auditorías, cortafuegos, Secure Boot, AppArmor, certificados, claves SSH/GPG y comprobaciones de integridad.
- **Exportaciones:** inventario TXT, CSV o JSON; informes del sistema TXT, CSV, JSON o PDF.

Algunas funciones dependen de las herramientas instaladas en la distribución. El tema y el idioma de la interfaz se pueden configurar.

## Instalación

Los paquetes precompilados están en [`dist/`](dist/), para las arquitecturas indicadas en sus nombres. Desde la raíz del repositorio, elige el formato de tu distribución.

| Distribución | Comando de instalación |
| --- | --- |
| Fedora, RHEL y compatibles | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian y Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux y derivados | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

El paquete Alpine incluido es un archivo local sin firmar por un repositorio configurado, por lo que se necesita `--allow-untrusted`. Los demás gestores instalan el archivo local y resuelven las dependencias declaradas. Polkit se necesita para las operaciones privilegiadas: el paquete Debian lo recomienda y los demás lo declaran como dependencia.

Tras instalarlo, ejecuta `orbisys` en una terminal o abre **OrbiSys** desde el menú de aplicaciones.

### Compilar desde el código fuente

Requisito: Rust y Cargo 1.80 o posterior. Desde la raíz del proyecto:

```sh
cargo build --release
./target/release/orbisys
```

Sin argumentos se abre la interfaz gráfica. Para la CLI: `./target/release/orbisys help`. En desarrollo, puedes usar, por ejemplo, `cargo run -- help`.

## Inicio rápido

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv programas.csv
orbisys updates
orbisys clean                 # analiza sin borrar nada
orbisys report ./informes
orbisys panel                 # lista las secciones
orbisys panel monitor
```

## Comandos CLI

| Comando | Descripción |
| --- | --- |
| `help`, `--help`, `-h` | Muestra la ayuda. |
| `version`, `--version` | Muestra la versión. |
| `inventory [--txt ARCHIVO] [--csv ARCHIVO] [--json ARCHIVO]` | Muestra el inventario y escribe las exportaciones solicitadas. |
| `updates` | Consulta actualizaciones con el gestor detectado. |
| `clean` | Muestra los elementos de limpieza sin borrarlos. |
| `clean --apply` | Limpia los elementos de usuario compatibles; omite los del sistema que requieren root. |
| `report [DIRECTORIO]` | Escribe informes TXT, CSV, JSON y PDF en el directorio indicado o en Documentos por defecto. |
| `panel` / `panel NOMBRE` | Lista las secciones / muestra una sección en modo de solo lectura. |

`orbisys panel` muestra los nombres válidos; por ejemplo, `monitor`, `health`, `mounts`, `lan` y `certs`.

## Seguridad y permisos

Las vistas de consulta y los comandos de diagnóstico no modifican el sistema. Las operaciones privilegiadas se delegan a Polkit (`pkexec`) y requieren autorización. En la CLI, `clean --apply` solo procesa elementos del usuario.

OrbiSys ejecuta programas con argumentos separados y valida los nombres de paquetes, unidades y rutas antes de usarlos. Las herramientas del sistema disponibles pueden variar según la distribución.

## Crear paquetes

Los scripts de [`packaging/`](packaging/) crean paquetes DEB, RPM, Arch y Alpine a partir del binario release:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Intentar todos los formatos
./packaging/build-all.sh
```

Herramientas necesarias: `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` y `zstd` (Arch), y `apk-tools` 3 o Docker (Alpine). Los paquetes se guardan en `dist/`. En algunos sistemas Debian/Ubuntu, el script RPM puede descargar sus herramientas localmente.

## Idiomas

La interfaz está disponible en francés, inglés, alemán, español, portugués, italiano, neerlandés, japonés, coreano, chino y árabe. Las traducciones francesa, inglesa y española están completas; las demás pueden ser parciales y los textos ausentes se muestran en inglés.

## Estructura del proyecto

- `src/main.rs`: punto de entrada y selección GUI/CLI.
- `src/lib.rs`: núcleo compartido y módulos del sistema.
- `src/cli.rs`: comandos CLI.
- `src/gui.rs`, `src/gui/`: interfaz gráfica.
- `src/p_*.rs`, `src/panels.rs`: paneles especializados.
- `src/lang/`: traducciones de la aplicación.
- `packaging/`: scripts y metadatos de paquetes.
- `assets/`: recursos gráficos; `dist/`: paquetes precompilados.

## Licencia

El proyecto declara la licencia MIT en [`Cargo.toml`](Cargo.toml). Los iconos Lucide incluidos tienen una licencia aparte en [`assets/icons/LICENSE`](assets/icons/LICENSE).
