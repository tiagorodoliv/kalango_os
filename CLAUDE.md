# Kalango OS — interface (shell) em Rust + Slint

Projeto da interface gráfica do **Kalango OS**, distro Linux baseada no Debian.
**Open source (GPL-3.0-or-later), decidido em 2026-11-12**; o repositório fica privado até a versão estável.
Nome e logo são marca protegida (`TRADEMARKS.md`). Contribuições por PR revisado pelo Tiago (`CONTRIBUTING.md`,
DCO com `git commit -s`); CI em `.github/workflows/ci.yml` e ISO publicada por tag `v*` (`release.yml`, Release
em rascunho). Doações: `.github/FUNDING.yml`, ainda todo comentado (plataforma não escolhida).
Comentários, docs e commits seguem as preferências globais do Tiago (pt-BR, Conventional Commits, JSDoc/rustdoc em tudo que é público).

## Decisões já tomadas

- **Base do sistema:** Debian 13 "trixie" (kernel Linux + userland Debian). ISO a ser gerada com `live-build`.
- **Linguagem/UI:** Rust + Slint (resolvido para **slint 1.18.1** no Cargo.lock).
  - Dev: janela comum no macOS (backend winit, renderer femtovg).
  - Dispositivo: `cargo build --release --features kms` (desenha direto via DRM/KMS).
- **Compositor (decidido em 2026-11-11): labwc + shell em Slint.** A sessão é greetd → `kalango-session` → labwc
  → `kalango-shell`. Sem layer-shell (winit não suporta), o shell roda em 3 processos do mesmo binário
  (`src/session.rs`), cada um com seu app_id e regra em `etc/xdg/labwc/rc.xml`:
  - `kalango-shell` (sem argumento): barra, dock e fundo; tela cheia, sem moldura, sempre atrás dos apps.
    O `<margin top="35" bottom="88">` do labwc impede app maximizado de cobrir barra e dock.
  - `kalango-shell --launcher`: menu de apps em tela cheia, transparente, sempre por cima. Super (onRelease) ou
    Super+Espaço abre; rodar de novo fecha (pidfile em `$XDG_RUNTIME_DIR`).
  - `kalango-shell --settings`: Configurações em janela comum. A aparência é salva em
    `~/.config/kalango/aparencia.conf` e os outros processos aplicam em <1 s (`src/appearance.rs`).
    As outras páginas mexem no sistema de verdade (`src/device.rs` + global `Device` em `ui/device.slint`,
    páginas em `ui/screens/settings_pages.slint`): Rede e Wi-Fi pelo `nmcli` (lista, liga/desliga, conecta com
    senha), Som pelo `wpctl` (volume/mudo), Energia (bateria, suspender/reiniciar), Usuários (`passwd` no foot),
    Atualizações e Sobre o sistema (os-release, kernel, CPU, memória, disco). Bluetooth e Monitores só têm aviso.
    O `nmcli` roda com `LC_ALL=C` (os estados viriam traduzidos). Antes de conectar com senha, o perfil antigo
    da rede é apagado: um perfil de tentativa com senha errada faz o `nmcli` falhar com "key-mgmt missing".
    Campo que precisa de `focus()` não pode estar sob `visible: false` nem num `if` recém-criado (o foco se
    perde no winit); o painel da senha do Wi-Fi encolhe para altura 0 com `opacity: 0`.
  - `kalango-shell --greeter`: tela de login de verdade (`src/greeter.rs`). O greetd roda `kalango-greeter`
    (labwc com config própria em `/etc/kalango/greeter`, sem atalhos) como `_greetd`; a senha vai pelo IPC do
    greetd (JSON com tamanho u32 nativo, socket `$GREETD_SOCK`) para o PAM e, se conferir, o greeter sai e o
    greetd inicia `kalango-session`. Usuário padrão: o primeiro de UID 1000–59999 do `/etc/passwd`;
    "Outro usuário" deixa digitar o login. Sem `$GREETD_SOCK` qualquer senha entra (dev).
- **Modo real × protótipo:** `KALANGO_SESSION=1` (em `etc/xdg/labwc/environment`) liga `Shell.native`: dock/menu
  abrem apps instalados (`session::command_for`: thunar, foot, firefox-esr, mousepad, ristretto, htop, nmtui,
  pavucontrol, parole, gnome-software = Loja, gnome-calendar = Agenda), avisos vão para o centro da barra (o rodapé pode estar atrás de um app), power faz
  `systemctl poweroff`, a área de trabalho começa direto (sem login falso) e o nome vem do GECOS de `$USER`.
  Rodapé do menu: cadeado = `swaylock` (ext-session-lock, seguro), sair = `labwc --exit` (volta ao greeter).
  "Todos os apps" do menu = os `.desktop` do sistema (`src/desktop.rs`: `Name[pt_BR]`, ícone do hicolor/Adwaita
  ou a inicial, `Exec` sem `%f`/`%U`, `Terminal=true` abre no foot; some `NoDisplay`, `OnlyShowIn` e o
  instalador do Debian), id `desktop:<caminho>`; a busca procura neles depois dos fixados, sem repetir os
  programas dos fixados. Lidos numa thread depois de o menu aparecer (~40 ms com 60 apps em release).
  RECENTES do menu = `~/.local/share/recently-used.xbel` (`src/recent.rs`), abertos com `xdg-open`; na ISO live
  (`/run/live/medium` existe) o 1º item é "Instalar Kalango OS" (app `installer`).
  Sem a variável (dev no host, `examples/snapshot.rs`) continua o protótipo com as janelas de demonstração.
- **Instalador: Calamares** com a sequência do `calamares-settings-debian` e, por cima (`etc/calamares/`): marca
  `branding/kalango` (logo, slides, `stylesheet.qss` escuro), `locale.conf` (São Paulo/pt-BR), `packages.conf`
  (remove também o calamares) e `shellprocess@kalango` (tira o `[initial_session]` do greetd e o
  `kalango-instalar`; **não usar `$` nos comandos**, o Calamares trata `$nome` como variável).
  `kalango-instalar` abre o Calamares com `sudo env WAYLAND_DISPLAY=... calamares` (a sessão não tem agente do
  polkit para o pkexec). O GRUB vai por apt a partir da mídia: `package-lists/kalango.list.binary` põe grub-pc,
  grub-efi e cryptsetup no repositório da ISO. Testado em VM: instala, dá boot pelo disco e entra no greeter.
- Identidade: `/usr/lib/os-release` = "Kalango OS" (`ID=kalango`, `ID_LIKE=debian`, protegido de atualização do
  base-files pelo hook `0050`), `etc/default/grub.d/kalango.cfg` (`GRUB_DISTRIBUTOR`).
- Próximos passos da arquitetura: layer-shell para barra/dock/menu; contraste das legendas de partição do
  Calamares no tema escuro (desenhadas pela paleta Qt, não pelo qss).

## Ambiente

- Rust instalado via rustup em `~/.cargo` **sem** alterar o PATH: usar `~/.cargo/bin/cargo` ou rodar `source ~/.cargo/env`.
- O esqueleto atual compila (`cargo build` OK).
- Máquina Linux de dev (Ubuntu 26.10, x86_64): `cargo build`, `cargo test` e `cargo run --example snapshot` OK.
  Pacotes do host já instalados: `libudev-dev libinput-dev libdrm-dev libgbm-dev libseat-dev debootstrap
  debian-archive-keyring` (+ qemu-system-x86, OVMF; usuário no grupo `kvm`). `--features kms` compila.
- glibc do host (Ubuntu, 2.44) é mais nova que a do trixie (2.41): **não** copiar binário compilado no host para a ISO.
  O `live-build` do Ubuntu é a 3.0 (2012) e não serve. Por isso `distro/build.sh` usa um chroot trixie
  (`distro/.cache/builder`) que compila o shell e roda o live-build do Debian. Nunca dar `rm -rf` nesse chroot
  com o script rodando: /dev, /proc e /sys ficam montados dentro dele.
  O `lb build` às vezes falha com `umount: .../chroot/proc: target is busy` (corrida com o apt saindo);
  o `build.sh` tenta de novo até 3 vezes (o live-build retoma de onde parou) e, ao sair, desmonta o que
  sobrou no chroot da ISO. Não editar o `build.sh` com ele rodando: o sh lê o script aos poucos.

## ISO (`distro/`)

- `distro/build.sh` (pede sudo) → `distro/out/kalango-os-<versão>.iso` (versão do `Cargo.toml`); `--so-shell` só compila o binário.
- `distro/run-qemu.sh [--uefi] [--gl]` testa a ISO no QEMU/KVM (q35, `vmport=off`, 1440×900). Padrão: GTK sem GL
  (`gtk,gl=on` no GNOME/Wayland do host inverte o mouse e estraga a imagem); `--gl` usa virgl em janela SDL.
- Em VM, `/usr/local/bin/kalango-session` exporta `WLR_RENDERER=pixman` e `WLR_NO_HARDWARE_CURSORS=1`: com o labwc
  em GL na GPU virtual aparecem blocos pretos no canto superior esquerdo.
- Testar sem sudo/sem refazer a ISO: compilar contra o trixie com `bwrap --ro-bind distro/.cache/builder /` +
  toolchain do host (binário em `target/release/kalango_os`), passar para a VM por `-virtfs` (9p) e, dentro dela,
  copiar binário/configs, `rm /run/greetd.run` (o greetd só faz autologin uma vez por boot) e
  `systemctl restart greetd`. A VM live tem rede e sudo sem senha (dá para `apt install`).
  Wi-Fi de teste na VM: `modprobe mac80211_hwsim radios=3`, `nmcli dev set wlan1 managed no` (idem wlan2),
  `hostapd` em wlan1 (WPA2) e wlan2 (aberta) e `dnsmasq` como DHCP; o wlan0 fica para o NetworkManager.
- Menu de boot com timeout de 5 s: `distro/config/bootloaders/{isolinux/isolinux.cfg,grub-pc/config.cfg}`
  (o padrão do live-build é esperar o Enter para sempre).
- Atalhos (labwc, além dos padrão): Super = menu, Super+Enter = foot, Super+E = thunar, Super+B = firefox,
  Super+L = bloquear (swaylock; `/etc/swaylock/config` com as cores do tema e textos invisíveis, porque o
  swaylock só fala inglês).
- Live: autologin (`initial_session`) uma vez por boot; ao sair aparece o greeter. Usuário `user`, senha `live`,
  nome exibido "Usuário" (`user-fullname` no `auto/config`).
  Para depurar, Ctrl+Alt+F2 dá console com login automático (usuário `user`, senha `live`);
  `journalctl -t kalango-shell` tem a saída do shell. O greetd está no VT7.
- Tema escuro dos apps: `etc/gtk-3.0/settings.ini`, `etc/gtk-4.0/settings.ini`, `etc/xdg/labwc/themerc-override`,
  `etc/xdg/foot/foot.ini` (mesmas cores de `ui/theme.slint`); libadwaita (Loja, Agenda) só lê a preferência pelo
  portal, que a sessão não tem: `ADW_DEBUG_COLOR_SCHEME=prefer-dark` no `environment` do labwc (+ override de
  GSettings `90_kalango.gschema.override`, compilado pelo hook `0100`).
- `run-qemu.sh --disco` liga um disco de 20 GB (`distro/out/disco-teste.qcow2`) para testar o instalador;
  `--so-disco` dá boot pelo disco sem a ISO.
- `auto/config` usa `noeject` (desligar não para pedindo para remover a mídia).
- `distro/auto/config`: trixie amd64, main+contrib+non-free-firmware, pt_BR, teclado br, usuário live `user`.
- Pacotes em `distro/config/package-lists/kalango.list.chroot`; configs de sistema em `distro/config/includes.chroot/`.

## Assets (já organizados)

- `assets/layout/` — telas de referência 2880×1800: `login.png`, `desktop.png`, `menu_apps.png`, `config_aparencia.png`, `logotipo.png`.
- `assets/wallpapers/` — 6 wallpapers 2880×1800: Ciano, Roxo, Azul, Verde, Amarelo, Rosa
  (mesma ordem das 6 cores de destaque da tela Aparência → trocar a cor de destaque pode trocar o wallpaper).
- `assets/logo/` — `Kalango_Logo_Vetorizado.svg` e `Kalango_Logo_transparente.png` (2172×724).
- `ui/icons/` — 29 ícones SVG de traço (24×24, stroke branco; aplicar cor com `colorize`):
  folder, terminal, globe, code, bag, settings, image, music, file-text, file, calendar, activity, download,
  search, lock, power, wifi, volume, battery, x, chevron-left/right, arrow-right, user, accessibility,
  monitor, bluetooth, info, contrast.
- Na raiz ainda estão os arquivos originais enviados (zips, logo .svg/.png duplicados). Não apagar sem perguntar ao Tiago.

## Especificação extraída do layout

Escala: layout em 2880×1800 físico; janela de referência **1440×900 lógico** (medidas abaixo já em px lógicos).

**Cores (tema Escuro/vidro):** fundo `#07090d`, painel/janela `#0d1218`, tile `#131920`, borda `#1e2a33`,
texto `#e6edf3`, texto secundário `#8b98a5`, perigo (power) `#f07178`, linhas de circuito = destaque com ~35% alfa,
grade de fundo sutil (~48px).
**Cores de destaque (ordem):** ciano `#3dd6f5`, roxo `#a78bfa`, azul `#7ea6f7`, verde `#5fd38d`, âmbar `#e8b04b`, rosa `#f28b95`.
**Fontes:** sans tipo Inter para UI; mono tipo JetBrains Mono para relógio, barra, rótulos de seção (MAIÚSCULAS espaçadas) e terminal.
**Marca:** usar os arquivos reais da marca em `ui/brand/` (global `Brand` em `ui/brand.slint`), nunca desenho próprio.
Ícone (barra, terminal, janela): `kalango-icon.png`, o "g"-calango no quadrado azul, recortado de `assets/layout/logotipo.png`.
Logotipo: `kalango-logo.png` (letras brancas) e `kalango-logo-claro.png` (letras azul-marinho, tema claro).
O SVG de `assets/logo` é rastreado de bitmap e fica granulado em tamanho pequeno; só trocar para vetor se vier um desenhado à mão.

1. **Tela de login:** relógio grande mono (~108px) "19:38" + "quarta-feira, 11 de novembro" no topo; card central
   ~380×320 (avatar círculo com "U" e borda destaque, "Usuário", campo "Senha" com botão seta em destaque,
   link "Outro usuário", rótulo "wayland"); hexágono e linhas de circuito no fundo; rodapé: marca + "Kalango OS"
   à esquerda, botões PT-BR / acessibilidade / power (vermelho) à direita.
2. **Área de trabalho:** barra superior ~35px (pílula "Kalango" com marca; indicador de workspaces: barra destaque + 2 pontos;
   data/hora central mono "qua 11 nov 19:38"; à direita CPU %, wifi, volume, bateria %, power). Janela Arquivos
   (~x112 y97 760×500: voltar/avançar, caminho `~/Projetos`, sidebar LOCAIS/DISPOSITIVOS, pastas firmware/web/app/distro + README.md)
   e janela Terminal (~x650 y301 641×400, borda destaque, bolinhas âmbar/verde/vermelho, saída de fastfetch com a marca).
   Dock inferior centralizada: Arquivos, Terminal (ativo, traço destaque embaixo), Navegador, Código, Loja | Configurações.
3. **Menu de apps** (Super ou clique na pílula, que fica preenchida com o destaque): card ~759×569 centralizado;
   busca "Buscar apps, arquivos, comandos..." com etiqueta "Super"; FIXADOS (6×2: Arquivos, Terminal, Navegador, Código,
   Loja, Configurações, Imagens, Música, Documentos, Agenda, Monitor, Atualizações) + link "Todos os apps";
   RECENTES (README.md ~/Projetos/distro, main.cpp ~/Projetos/firmware); rodapé avatar "Usuário" + bloquear + power.
4. **Configurações → Aparência:** sidebar ~280px (título, busca, itens: Rede e Wi-Fi, Bluetooth, Aparência [selecionado],
   Monitores, Som, Energia, Usuários, Atualizações [badge âmbar 3], Sobre o sistema). Conteúdo: TEMA com 3 cards de
   prévia (Escuro (vidro), Escuro sólido (OLED), Claro); COR DE DESTAQUE (6 círculos); painel com slider
   "Transparência das janelas" (60%) e toggles "Desfoque do fundo", "Dock sempre visível", "Reduzir animações". Botão X fecha.

## Próximo passo (plano que estava em andamento)

1. `ui/theme.slint`: global `Theme` com `accent-index`, `mode` (enum glass/oled/light), transparência, toggles,
   e cores derivadas (tudo reativo → a tela Aparência altera o sistema inteiro ao vivo).
2. `ui/apps.slint`: struct `AppEntry { id, name, icon, tint }` + global `Apps` com os 12 fixados.
3. Componentes: `KalangoMark` (Path), `IconButton`, `Toggle`, `SliderBar`, `TopBar`, `Dock`, `Backdrop` (grade + circuitos
   via Path em viewbox 2000×1250), `AppFrame` (janela arrastável pela barra de título).
4. Telas: `login`, `desktop` (+ mocks de Arquivos/Terminal), `launcher` (overlay), `settings`.
5. Rust: mover a lógica para `src/lib.rs`; relógio local com nomes pt-BR (crate `chrono`); CPU via `/proc/stat` e bateria
   via `/sys/class/power_supply` (mostrar "--" fora do Linux); busca do launcher filtrada em Rust; login aceita senha
   não vazia no protótipo (no sistema real: PAM via greetd); power encerra o protótipo (real: logind).
6. Ferramenta de prévia: `examples/snapshot.rs` com renderer de software para gerar PNG de cada tela e comparar com
   `assets/layout/` (pode exigir a feature `software-renderer-systemfonts`; se Path não renderizar, usar `screencapture`).
