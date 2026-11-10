# OrbiSys

**Manutenção e administração Linux em uma única aplicação.** OrbiSys reúne uma interface gráfica e uma CLI baseadas no mesmo núcleo Rust.

[![Versão](https://img.shields.io/badge/version-1.0.0-2f6f62)](Cargo.toml) [![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange)](https://www.rust-lang.org/) [![Licença MIT](https://img.shields.io/badge/license-MIT-blue)](Cargo.toml)

<p align="center"><img src="packaging/orbisys.svg" alt="Logotipo OrbiSys" width="112"></p>

**Idiomas:** [English](README.md) · [Français](README.fr.md) · [Deutsch](README.de.md) · [Español](README.es.md) · Português · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [中文](README.zh.md) · [العربية](README.ar.md)

## Recursos

O OrbiSys reúne ferramentas de consulta e manutenção do sistema:

- **Sistema e monitoramento:** estado do computador, processos, serviços, registos, inicialização e tarefas agendadas.
- **Software:** inventário de pacotes nativos, Snap, Flatpak e aplicações instaladas manualmente; atualizações, repositórios e ambientes de execução.
- **Armazenamento e cópias de segurança:** discos, montagens, limpeza, arquivos, Btrfs, instantâneos e backups.
- **Rede e hardware:** rede local, tráfego, Wi-Fi, VPN, Bluetooth, USB, energia, sensores e firmware.
- **Segurança:** auditorias, firewall, Secure Boot, AppArmor, certificados, chaves SSH/GPG e verificações de integridade.
- **Exportações:** inventário em TXT, CSV ou JSON; relatórios do sistema em TXT, CSV, JSON ou PDF.

Alguns recursos dependem das ferramentas instaladas na distribuição. O tema e o idioma da interface são configuráveis.

## Instalação

Pacotes pré-compilados estão em [`dist/`](dist/) para as arquiteturas indicadas nos nomes dos ficheiros. Na raiz do repositório, escolha o formato da sua distribuição.

| Distribuição | Comando de instalação |
| --- | --- |
| Fedora, RHEL e compatíveis | `sudo dnf install ./dist/orbisys-*.rpm` |
| openSUSE | `sudo zypper install ./dist/orbisys-*.rpm` |
| Debian e Ubuntu | `sudo apt install ./dist/orbisys_*_*.deb` |
| Arch Linux e derivados | `sudo pacman -U ./dist/orbisys-*.pkg.tar.zst` |
| Alpine Linux | `sudo apk add --allow-untrusted ./dist/orbisys-*.apk` |

O pacote Alpine incluído é um ficheiro local não assinado por um repositório configurado, por isso é necessário `--allow-untrusted`. Os outros gestores instalam o ficheiro local e resolvem as dependências declaradas. Polkit é necessário para operações privilegiadas: o pacote Debian recomenda-o e os restantes formatos declaram-no como dependência.

Depois da instalação, execute `orbisys` no terminal ou abra **OrbiSys** no menu de aplicações.

### Compilar a partir do código-fonte

Requisito: Rust e Cargo 1.80 ou posterior. Na raiz do projeto:

```sh
cargo build --release
./target/release/orbisys
```

Sem argumentos, o binário abre a interface gráfica. Para a CLI: `./target/release/orbisys help`. Durante o desenvolvimento, por exemplo: `cargo run -- help`.

## Início rápido

```sh
orbisys help
orbisys version
orbisys inventory
orbisys inventory --csv software.csv
orbisys updates
orbisys clean                 # analisa sem apagar
orbisys report ./relatorios
orbisys panel                 # lista as secções
orbisys panel monitor
```

## Comandos CLI

| Comando | Descrição |
| --- | --- |
| `help`, `--help`, `-h` | Mostra a ajuda. |
| `version`, `--version` | Mostra a versão. |
| `inventory [--txt FICHEIRO] [--csv FICHEIRO] [--json FICHEIRO]` | Mostra o inventário e grava as exportações solicitadas. |
| `updates` | Consulta atualizações através do gestor detetado. |
| `clean` | Mostra os alvos de limpeza sem apagar nada. |
| `clean --apply` | Limpa alvos pessoais compatíveis; ignora os alvos de sistema que exigem root. |
| `report [DIRETÓRIO]` | Grava relatórios TXT, CSV, JSON e PDF no diretório indicado ou, por omissão, em Documentos. |
| `panel` / `panel NOME` | Lista as secções / mostra uma secção em modo só de leitura. |

`orbisys panel` mostra os nomes aceites, por exemplo `monitor`, `health`, `mounts`, `lan` e `certs`.

## Segurança e permissões

Os ecrãs de consulta e os comandos de diagnóstico não alteram o sistema. As operações privilegiadas são delegadas ao Polkit (`pkexec`) e exigem autorização. Na CLI, `clean --apply` processa apenas alvos pessoais.

O OrbiSys executa programas com argumentos separados e valida nomes de pacotes, unidades e caminhos antes de os usar. As ferramentas disponíveis variam conforme a distribuição.

## Criar pacotes

Os scripts de [`packaging/`](packaging/) criam pacotes DEB, RPM, Arch e Alpine a partir do binário de release:

```sh
cargo build --release
./packaging/build-deb.sh
./packaging/build-rpm.sh
./packaging/build-arch.sh
./packaging/build-apk.sh
# Tentar todos os formatos
./packaging/build-all.sh
```

Ferramentas necessárias: `dpkg-deb` (DEB), `rpmbuild` (RPM), `tar` e `zstd` (Arch), e `apk-tools` 3 ou Docker (Alpine). Os pacotes são guardados em `dist/`. Em alguns sistemas Debian/Ubuntu, o script RPM pode obter as ferramentas localmente.

## Idiomas

A interface está disponível em francês, inglês, alemão, espanhol, português, italiano, neerlandês, japonês, coreano, chinês e árabe. As traduções francesa, inglesa e espanhola estão completas; as restantes podem ser parciais e os textos em falta recorrem ao inglês.

## Estrutura do projeto

- `src/main.rs`: ponto de entrada e seleção GUI/CLI.
- `src/lib.rs`: núcleo partilhado e módulos do sistema.
- `src/cli.rs`: comandos CLI.
- `src/gui.rs`, `src/gui/`: interface gráfica.
- `src/p_*.rs`, `src/panels.rs`: painéis especializados.
- `src/lang/`: traduções da aplicação.
- `packaging/`: scripts e metadados dos pacotes.
- `assets/`: recursos gráficos; `dist/`: pacotes pré-compilados.

## Licença

O projeto declara a licença MIT em [`Cargo.toml`](Cargo.toml). Os ícones Lucide incluídos têm uma licença separada em [`assets/icons/LICENSE`](assets/icons/LICENSE).
