# kalango_os

Interface gráfica (shell) do sistema operacional proprietário, em **Rust + [Slint](https://slint.dev)**.

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
  clock.rs             data/hora local em pt-BR
  system.rs            CPU (/proc/stat) e bateria (/sys/class/power_supply)
  apps.rs              busca do launcher (ignora acentos e maiúsculas)
  session.rs           sessão real: abrir apps, menu e Configurações como janelas próprias, desligar
  appearance.rs        aparência salva em ~/.config/kalango e compartilhada entre os processos
  greeter.rs           tela de login de verdade (greeter do greetd, senha conferida no PAM)
  recent.rs            arquivos recentes de verdade no menu de apps
examples/
  snapshot.rs          gera PNG de cada tela em target/snapshots/
ui/
  app_window.slint     janela raiz e navegação entre telas
  theme.slint          tema reativo (destaque, modo, transparência, toggles)
  globals.slint        catálogo de apps e dados do sistema
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

Para instalar: menu de apps → RECENTES → **Instalar Kalango OS** (Calamares, com a marca do Kalango).

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
