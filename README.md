# linux_os

Interface gráfica (shell) do sistema operacional proprietário, em **Rust + [Slint](https://slint.dev)**.

## Por que Rust + Slint

- Binário nativo, pequeno e sem coletor de lixo: inicia rápido e usa pouca RAM.
- O Slint desenha direto na tela via **DRM/KMS** (sem X11/Wayland), ideal para um SO próprio.
- O layout fica em arquivos `.slint` declarativos, separados da lógica em Rust.
- Em desenvolvimento roda numa janela comum no macOS/Linux.

## Estrutura

```
Cargo.toml           dependências e feature `kms` (build do dispositivo)
build.rs             compila ui/*.slint em código Rust
src/
  main.rs            ponto de entrada: cria a janela e liga os serviços
  clock.rs           relógio da barra de status
ui/
  app_window.slint   janela raiz
  theme.slint        cores, espaçamentos e fontes (tokens do layout)
  components/        componentes reutilizáveis (barra de status, botões...)
  screens/           telas (home, configurações...)
assets/              ícones, imagens e fontes
```

## Pré-requisitos

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Rodar em desenvolvimento

```sh
cargo run
```

## Build para o dispositivo (Linux, tela direta via KMS)

```sh
cargo build --release --features kms
```

O dispositivo precisa de `libinput`, `libudev`, `libxkbcommon` e acesso a `/dev/dri` e `/dev/input`.
Para compilação cruzada (ex.: ARM64), usar `cross`: `cross build --release --features kms --target aarch64-unknown-linux-gnu`.
