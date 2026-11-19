# KalangOS

Distro Linux baseada no **Debian 13 (trixie)**, com uma interface própria (shell) em
**Rust + [Slint](https://slint.dev)** sobre o compositor Wayland **labwc**.

Software livre sob a [GPL-3.0](LICENSE). Quer ajudar? Veja [CONTRIBUTING.md](CONTRIBUTING.md).

## Documentação

Para entender e manter o projeto (arquitetura, Rust e Slint, mapa do código,
sistema e hardware, pacotes e atualizações, ISO, segurança e rotina de
manutenção), comece por [`docs/README.md`](docs/README.md).

## Por que Rust + Slint

- Binário nativo, pequeno e sem coletor de lixo: inicia rápido e usa pouca RAM.
- O Slint desenha direto na tela via **DRM/KMS** (sem X11/Wayland), ideal para um SO próprio.
- O layout fica em arquivos `.slint` declarativos, separados da lógica em Rust.
- Em desenvolvimento roda numa janela comum no macOS/Linux.

## Estrutura

```
Cargo.toml             dependências e feature `kms` (build do dispositivo)
build.rs               compila ui/app_window.slint em código Rust
src/
  main.rs              ponto de entrada (só chama kalango_os::run)
  lib.rs               monta a janela e liga os serviços
  clock.rs             data/hora local (inglês ou português)
  system.rs            CPU (/proc/stat) e bateria (/sys/class/power_supply)
  apps.rs              busca do launcher (ignora acentos e maiúsculas) e "Todos os apps"
  desktop.rs           apps instalados, lidos dos arquivos .desktop (nome no idioma, ícone, Exec)
  session.rs           sessão real: abrir apps, menu e Configurações como janelas próprias, desligar
  appearance.rs        aparência salva em ~/.config/kalango e compartilhada entre os processos
  greeter.rs           tela de login de verdade (greeter do greetd, senha conferida no PAM)
  recent.rs            arquivos recentes de verdade no menu de apps
  device.rs            Configurações reais: Wi-Fi (nmcli), som (wpctl), energia e ficha do sistema
  displays.rs          Configurações → Monitores: resolução, escala e rotação (wlr-randr)
  labwc.rs             margem da dock no labwc (Configurações → Barra de tarefas)
  i18n.rs              idioma da interface (inglês padrão, português pelo LANG)
  region.rs            Configurações → Idioma e região: idioma e teclado
lang/pt_BR/LC_MESSAGES/kalango_os.po   tradução para o português (textos @tr dos .slint)
examples/
  snapshot.rs          gera PNG de cada tela em target/snapshots/
ui/
  app_window.slint     janela raiz e navegação entre telas
  theme.slint          tema reativo (destaque, modo, transparência, toggles)
  globals.slint        catálogo de apps e dados do sistema
  device.slint         global `Device`: rede, som, energia e ficha do sistema (preenchido por device.rs)
  icons.slint          ícones SVG de ui/icons/
  components/          marca, botões, toggle, slider, barra, dock, fundo, moldura de janela
  screens/             login, launcher (menu de apps), configurações
  apps/                mocks das janelas Arquivos e Terminal
  fonts/               Inter e JetBrains Mono (licença OFL, embutidas no binário)
assets/                layouts de referência, logo e wallpapers
```

## Como rodar

```sh
~/.cargo/bin/cargo run                       # abre o protótipo numa janela
~/.cargo/bin/cargo run --example snapshot    # gera as capturas das telas
~/.cargo/bin/cargo test                      # testes de relógio, CPU, busca, sessão e aparência
```

Qualquer senha não vazia entra. Atalhos: **Super** (no macOS, **⌃ Control**) abre/fecha o menu de apps;
**Esc** fecha o menu ou as Configurações.

## Pré-requisitos

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Rodar em desenvolvimento

```sh
cargo run
```

## Gerar a ISO (Debian 13 trixie + labwc)

```sh
distro/build.sh          # pede sudo; a primeira vez cria o chroot de build (~1 GB)
distro/run-qemu.sh       # testa a ISO gerada em distro/out/ no QEMU
```

`distro/run-qemu.sh --gl` usa aceleração 3D (janela SDL); `--uefi` dá boot por UEFI; `--disco` liga um disco
virtual para testar o instalador e `--so-disco` dá boot pelo sistema instalado, sem a ISO.

### Testar mudanças sem gerar a ISO

```sh
distro/testar-vm.sh            # compila o shell (~12 s depois da 1ª vez) e liga a VM já com ele
distro/testar-vm.sh --tudo     # idem, levando também papéis de parede e ícones
distro/testar-vm.sh --so-preparar   # com a VM ligada; depois, nela: sudo kalango-dev-aplicar --reiniciar
```

Não precisa de sudo: compila contra o Debian de build do `distro/build.sh` (precisa ter rodado uma vez) e
compartilha `target/vm-dev/` com a VM; o `kalango-dev.service` da ISO live aplica o shell e as configs antes da
tela de login. Pacotes novos e o instalador ainda precisam da ISO de verdade.

Para instalar: menu de apps → RECENTES → **Instalar KalangOS** (Calamares, com a marca do Kalango).

A Loja (GNOME Software) mostra os apps do Debian e do **Flathub** juntos (Spotify, Discord, VS Code, Steam...).

A ISO entra direto na sessão: greetd → labwc → `kalango-shell`. Logs do shell: `journalctl -t kalango-shell`.
Na ISO a dock e o menu abrem os apps de verdade (Thunar, foot, Firefox, Mousepad, Ristretto, Parole, htop,
GNOME Software como Loja, GNOME Agenda) e o
Wi-Fi e o volume da barra abrem `nmtui` e `pavucontrol`. Atalhos: **Super** abre/fecha o menu, **Super+Enter**
terminal, **Super+E** arquivos, **Super+B** navegador, **Super+L** bloqueia, **Alt+Tab** troca de janela,
**Alt+F4** fecha. Sair da sessão (rodapé do menu) leva à tela de login do Kalango: usuário `user`, senha `live`.

## Build para o dispositivo (Linux, tela direta via KMS)

```sh
cargo build --release --features kms
```

O dispositivo precisa de `libinput`, `libudev`, `libxkbcommon` e acesso a `/dev/dri` e `/dev/input`.
Para compilação cruzada (ex.: ARM64), usar `cross`: `cross build --release --features kms --target aarch64-unknown-linux-gnu`.

## Contribuir

Issues e Pull Requests são bem-vindos: o passo a passo, os padrões (pt-BR, Conventional Commits) e como
testar estão em [CONTRIBUTING.md](CONTRIBUTING.md). Toda mudança é revisada antes de entrar no `main`, e as
ISOs são publicadas em **Releases** quando o mantenedor cria uma tag de versão (`.github/workflows/release.yml`).
Falhas de segurança vão em particular: [SECURITY.md](SECURITY.md). Convivência: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Licença

O código do KalangOS (shell, scripts e configurações da distro) é licenciado sob a
**GNU General Public License v3.0 ou posterior** ([LICENSE](LICENSE)).

O nome "KalangOS" e o logotipo **não** estão sob a GPL: versões modificadas precisam de outro nome e outro
logo. Detalhes em [TRADEMARKS.md](TRADEMARKS.md).

Componentes de terceiros:
- [Slint](https://slint.dev), usado sob a licença GPL-3.0 (uma das licenças oferecidas pelo Slint);
- fontes Inter e JetBrains Mono, sob a SIL Open Font License (`ui/fonts/*LICENSE*`, `ui/fonts/*OFL*`);
- ícones [Flat Remix](https://github.com/daniruiz/flat-remix), de Daniel Ruiz de Alegría, sob a GPL-3.0
  (baixados no build por `distro/icones/flat-remix.sh`);
- ícones [Colloid](https://github.com/vinceliuice/Colloid-icon-theme) e
  [Fluent](https://github.com/vinceliuice/Fluent-icon-theme), de Vince Liuice, sob a GPL-3.0
  (baixados no build por `distro/icones/temas-extras.sh`);
- pacotes do Debian incluídos na ISO, cada um com a própria licença (em `/usr/share/doc/<pacote>/copyright`
  no sistema). O código-fonte deles está nos repositórios do Debian (`apt source <pacote>`) e em
  <https://sources.debian.org>.
