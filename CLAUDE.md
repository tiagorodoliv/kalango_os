# KalangOS — interface (shell) em Rust + Slint

Projeto da interface gráfica do **KalangOS**, distro Linux baseada no Debian.
**Nome: KalangOS** (decidido em 2026-11-15; antes "Kalango OS"; busca no INPI sem conflito nas classes 9/42, só
"Calango" de óculos na 9). Só o nome visível mudou: `ID=kalango`, `kalango-shell`, `org.kalango.*`, `kalango_os`, pastas e o
nome do arquivo da ISO continuam. Com `GRUB_DISTRIBUTOR="KalangOS"` a classe do GRUB é `kalangos` (ícone próprio no tema).
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
  - **Renderer na sessão (decidido em 2026-11-13, máquinas antigas):** software por padrão
    (`lib.rs::renderer_for`): em VM de 2 GB a área de trabalho caiu de 192 MB/1,4 s CPU por 30 s (femtovg pelo
    llvmpipe, que carrega o libLLVM) para 32 MB/0,08 s. Exceção: o menu (`--launcher`) usa femtovg, porque o
    renderer de software no Wayland não tem alfa (fundo preto). `KALANGO_RENDERER=gl|software` força;
    `$SLINT_BACKEND` tem prioridade. O renderer de software pinta `Path` por cima de `Image`: com papel de parede
    o `Backdrop` nem cria grade/circuitos. Sistema: zram (`systemd-zram-generator`) e `winbind` desligado.
- **Compositor (decidido em 2026-11-11): labwc + shell em Slint.** A sessão é greetd → `kalango-session` → labwc
  → `kalango-shell`. Sem layer-shell (winit não suporta), o shell roda em 3 processos do mesmo binário
  (`src/shell/session.rs`), cada um com seu app_id e regra em `etc/xdg/labwc/rc.xml`:
  - `kalango-shell` (sem argumento): barra, dock e fundo; tela cheia, sem moldura, sempre atrás dos apps.
    O `<margin top="35" bottom="70">` do labwc impede app maximizado de cobrir barra e dock (colada na borda).
  - `kalango-shell --launcher`: menu de apps em tela cheia, transparente, sempre por cima. Super (onRelease) ou
    Super+Espaço abre; rodar de novo fecha (pidfile em `$XDG_RUNTIME_DIR`).
  - `kalango-shell --settings`: Configurações em janela comum, com **moldura própria** (2026-11-15:
    `ui/components/window_frame.slint` + `install_window_frame` no `lib.rs`; regra `kalango-settings`
    `serverDecoration="no"` no rc.xml, porque o labwc 0.8 só arredonda a barra de título dele). Barra,
    botões no estilo/lado da Aparência, arrastar/redimensionar/maximizar pelo winit; cantos redondos só com
    OpenGL (alfa), retos maximizada. A aparência é salva em
    `~/.config/kalango/aparencia.conf` e os outros processos aplicam em <1 s (`src/settings/appearance.rs`).
    As outras páginas mexem no sistema de verdade (`src/settings/device.rs` + global `Device` em `ui/device.slint`,
    páginas em `ui/screens/settings_pages.slint`): Rede e Wi-Fi pelo `nmcli` (lista, liga/desliga, conecta com
    senha), Som pelo `wpctl` (volume/mudo; saída e entrada padrão por `wpctl status` + `set-default`, HDMI vira
    "HDMI · <monitor>" pelo `alsa.name` do EDID; setinha do volume no painel rápido lista as saídas), Energia (bateria, suspender/reiniciar), Usuários (`passwd` no foot),
    Atualizações e Sobre o sistema (os-release, kernel, CPU, memória, disco). Bluetooth só tem aviso.
    **Barra de tarefas** (opções no `Theme`, gravadas no mesmo `aparencia.conf`): dia/segundos/CPU/bateria na
    barra, dock sempre visível, tamanho (44/52/60 px) e apps da dock (`Theme.dock-apps`, ids por vírgula;
    `Apps.dock` = `Shell.dock-entries`). A área de trabalho ajusta a margem do labwc gerando
    `~/.config/labwc/rc.xml` (cópia do de /etc com a marca "Gerado pelo kalango-shell" e o `bottom` trocado;
    sem a marca, nunca sobrescreve) + `labwc --reconfigure` (`src/shell/labwc.rs`). **O labwc 0.8 relê o `<margin>` no
    reconfigure mas não recalcula a área útil** (só em mudança de monitor): quando a margem muda (e na 1ª vez de cada
    área de trabalho) o shell roda `wlr-randr --output X --on` 0,5 s depois (`labwc::refit_maximized_later`); o
    wlroots ignora os campos iguais (não pisca) e o labwc rearruma as janelas maximizadas. Janelas que ficam fora da
    área útil (dock, faixa) precisam de `fixedPosition="yes"` na regra, senão essa rearrumação as empurra. **O labwc só enxerga `~/.config/labwc` se a pasta já existir quando ele inicia**
    (por isso o `kalango-session` faz `mkdir -p` antes do `exec labwc`). A dock vai embaixo/esquerda/direita
    (`Theme.dock-position`) e como dock flutuante ou barra de ponta a ponta (`Theme.dock-style`); as margens
    viram `bottom`/`left`/`right` no `<margin>`. Papel de parede: `Theme.wallpaper` (0 = circuitos, padrão
    1 = PretoAzul), arquivos em `/usr/share/backgrounds/kalango` (copiados pelo `build.sh`), carregados por
    `src/shell/wallpaper.rs` na menor resolução que cobre a tela; escolha com miniaturas na Aparência. **Imagem própria** (`Theme.wallpaper` = 8 =
    `wallpaper::CUSTOM`, `papel-de-parede=arquivo` + `papel-de-parede-arquivo=`): "+" na Aparência (zenity) ou "Abrir
    com → Definir como papel de parede" (`kalango-papel-de-parede.desktop` → `kalango-shell --papel-de-parede ARQ`);
    o shell grava uma cópia reduzida (cobre 2560×1440, JPEG, crate `image` que o Slint já traz) e a miniatura em
    `~/.local/share/backgrounds`. A tela de bloqueio usa a mesma cópia. Tema de ícones novo: a dock em janela
    própria e o menu também recarregam (`apps::reload_icons`; o tema guardado em `desktop::current_icon_theme`
    é trocável). **Monitores** (`src/settings/displays.rs`): `wlr-randr` (texto, não `--json`),
    resolução/taxa/escala/rotação, confirmação de 15 s que volta sozinha, salvo em
    `~/.config/kalango/monitores.conf` e reaplicado pela área de trabalho ao entrar. Escala só se sobrar
    ≥ 1024×600 lógicos para o shell. `--mode` precisa da taxa exatamente como o wlr-randr escreve.
    O `nmcli` roda com `LC_ALL=C` (os estados viriam traduzidos). Antes de conectar com senha, o perfil antigo
    da rede é apagado: um perfil de tentativa com senha errada faz o `nmcli` falhar com "key-mgmt missing".
    Campo que precisa de `focus()` não pode estar sob `visible: false` nem num `if` recém-criado (o foco se
    perde no winit); o painel da senha do Wi-Fi encolhe para altura 0 com `opacity: 0`.
  - `kalango-shell --calculadora` (2026-11-15): **Calculadora** própria (`src/tools/`, `ui/screens/calculator.slint`,
    global `Calc`), moldura própria (regra `kalango-calculadora` no rc.xml), `.desktop` em "Todos os apps". Abas:
    Calculadora (`tools::expr`, resultado ao vivo, `ans`, histórico em `~/.local/state/kalango/calculadora.txt`),
    Programador (`eval_int`, 8–64 bits, com/sem sinal, bits clicáveis) e Ferramentas (`tools::codec`: Base64, URL,
    MD5/SHA-1/SHA-256 próprios testados com os vetores oficiais, tempo Unix, cores, UUID, JSON, texto). Sem
    dependências novas. Copiar = TextInput invisível com `copy()`.
  - `kalango-shell --quick-settings`: **painel rápido** do canto da barra (como o menu do sistema do Ubuntu;
    `ui/screens/quick_settings.slint`). Clicar no bloco Wi-Fi/volume/bateria/energia da barra abre; rodar de novo
    fecha (pidfile, como o launcher). Tela cheia, transparente, por cima (regra no rc.xml), renderer femtovg.
    Volume/mudo, brilho (só com `/sys/class/backlight`; `SetBrightness` do logind via `busctl`, sem root),
    Wi-Fi liga/desliga (sem placa: botão "Rede"), estilo escuro, bateria, Configurações, bloquear e submenu
    suspender/reiniciar/desligar/sair. No QEMU, voltar da suspensão reinicia a VM (limitação do QEMU).
  - `kalango-shell --greeter`: tela de login de verdade (`src/shell/greeter.rs`). O greetd roda `kalango-greeter`
    (labwc com config própria em `/etc/kalango/greeter`, sem atalhos) como `_greetd`; a senha vai pelo IPC do
    greetd (JSON com tamanho u32 nativo, socket `$GREETD_SOCK`) para o PAM e, se conferir, o greeter sai e o
    greetd inicia `kalango-session`. Usuário padrão: o primeiro de UID 1000–59999 do `/etc/passwd`;
    "Outro usuário" deixa digitar o login. Sem `$GREETD_SOCK` qualquer senha entra (dev).
- **Idioma (decidido em 2026-11-13): inglês por padrão, português como tradução**, para servir a outros países.
  Textos dos `.slint` em inglês com `@tr("...")`; tradução em `lang/pt_BR/LC_MESSAGES/kalango_os.po` (msgid =
  inglês, sem contexto: `DefaultTranslationContext::None` no `build.rs`), embutida no binário. Textos montados no
  Rust usam `i18n::tr("en", "pt")`. Idioma = `LC_ALL`/`LC_MESSAGES`/`LANG` começando com `pt`
  (`src/common/i18n.rs`). Texto novo na UI: `@tr("English")` + par msgid/msgstr no `.po`. Comentários e docs continuam
  em pt-BR. Configurações → **Idioma e região** (`src/settings/region.rs`): idioma (vale no próximo login) e teclado
  (na hora: `~/.config/labwc/environment` com marca + `labwc --reconfigure`), gravados em
  `~/.config/kalango/regiao.conf`, lido por `/usr/local/lib/kalango/regiao.sh` (kalango-session, greeter,
  instalador) depois de `/etc/default/locale` e `/etc/default/keyboard`. **O labwc lê um só `environment`**
  (o do usuário esconde o do sistema): por isso as variáveis da sessão são exportadas no `kalango-session`.
  Live: `locales=en_US.UTF-8,pt_BR.UTF-8 keyboard-layouts=us timezone=Etc/UTC`; Calamares começa em Nova York.
- **Modo real × protótipo:** `KALANGO_SESSION=1` (exportado pelo `kalango-session`) liga `Shell.native`: dock/menu
  abrem apps instalados (`session::command_for`: thunar, foot, firefox-esr, mousepad, ristretto, htop, nmtui,
  pavucontrol, parole, gnome-software = Loja, gnome-calendar = Agenda), avisos vão para o centro da barra (o rodapé pode estar atrás de um app), power faz
  `systemctl poweroff`, a área de trabalho começa direto (sem login falso) e o nome vem do GECOS de `$USER`.
  Rodapé do menu: cadeado = `swaylock` (ext-session-lock, seguro), sair = `labwc --exit` (volta ao greeter).
  "Todos os apps" do menu = os `.desktop` do sistema (`src/launcher/desktop.rs`: `Name[pt_BR]`, ícone do hicolor/Adwaita
  ou a inicial, `Exec` sem `%f`/`%U`, `Terminal=true` abre no foot; some `NoDisplay`, `OnlyShowIn` e o
  instalador do Debian), id `desktop:<caminho>`; a busca procura neles depois dos fixados, sem repetir os
  programas dos fixados. Lidos numa thread depois de o menu aparecer (~40 ms com 60 apps em release).
  RECENTES do menu = `~/.local/share/recently-used.xbel` (`src/launcher/recent.rs`), abertos com `xdg-open`; na ISO live
  (`/run/live/medium` existe) o 1º item é "Instalar KalangOS" (app `installer`).
  Sem a variável (dev no host, `examples/snapshot.rs`) continua o protótipo com as janelas de demonstração.
- **Rede local e compartilhamento (2026-11-13):** Configurações → **Sharing** (`src/settings/sharing.rs`, `SharingPage`).
  Compartilhar = Samba + `wsdd2` (aparece na Rede do Windows), **desligado por padrão** (hook `0120` desliga
  smbd/nmbd/wsdd2, tira `[homes]`/`[printers]`/`[print$]` do smb.conf e inclui `/etc/samba/kalango.conf`). Root só
  por `pkexec /usr/local/sbin/kalango-compartilhar ligar|desligar|pastas SEM_SENHA PASTA...` (política
  `org.kalango.compartilhar`, `auth_admin_keep`; só aceita pastas na casa de quem chamou). Shares: `read only` +
  `write list = <usuário>` + `force user`; sem senha = `guest ok` (visitante só lê); senha de rede = `smbpasswd`
  no foot. Escolha em `~/.config/kalango/compartilhamento.conf` (padrão: Public, sem senha). Computadores =
  `avahi-browse -tpr _smb._tcp` (Linux/macOS/NAS; Windows não se anuncia: campo de endereço `\\pc\pasta` →
  `smb://` e "Browse network" = `thunar network:///`), tudo pelo gvfs (`gvfs-backends`). Testado na VM: visitante
  lê e não grava, com senha grava, Thunar lista as pastas e a Rede.
- **Bluetooth, IP fixo e VPN (2026-11-13), como no Ubuntu:** `src/settings/bluetooth.rs` (`bluetoothctl`; parear numa sessão
  interativa com agente KeyboardDisplay, código do aparelho na tela; botão no painel rápido) e `src/settings/netconfig.rs`
  (Rede → CONEXÕES → "Configurar IP" pelo `nmcli connection modify`; VPN: importar `.ovpn`/`.conf`, formulários
  OpenVPN e WireGuard). **WireGuard nunca pelo `nmcli connection import`** (liga na hora; com 0.0.0.0/0 derruba a
  rede) nem pelo nmcli (não edita `peers`): o shell lê o `.conf` e cria por `gdbus` → `Settings.AddConnection`,
  desligado. Pacotes: bluez, libspa-0.2-bluetooth, network-manager-openvpn, wireguard-tools, zenity, libglib2.0-bin.
- **Máquina física do Tiago (notebook HP, 1366×768, dual boot):** atualizar o shell sem formatar por SSH
  (endereço e chave em `CLAUDE.local.md`, fora do git): `distro/testar-vm.sh --so-preparar`, copiar
  `target/vm-dev/kalango-shell` para `/usr/local/bin/` (guardar o antigo como `.anterior`) e, se a área de trabalho
  mudou, reiniciá-la **com o ambiente dela mesma** (`/proc/<pid>/environ` do `kalango-shell` sem argumento, que o
  labwc iniciou com `DISPLAY`, `LABWC_PID` e `WAYLAND_DISPLAY`) e no mesmo cgroup (`session-N.scope`, ver
  `/proc/<pid>/cgroup`). Sem `DISPLAY` os apps X11 da dock falham (VS Code), sem `LABWC_PID` a margem da dock não
  atualiza, fora da sessão o polkit nega o NetworkManager. Conferir antes se só há um `kalango-shell` sem argumento.
  `grim` instalado lá para capturas.
- **Áreas de trabalho e encaixe (2026-11-13):** 4 áreas (`<desktops>` no rc.xml), indicador real na barra pelo
  `ext-workspace-v1` do labwc 0.8.3 (`src/shell/workspaces.rs`, conexão Wayland própria numa thread; clicar ativa). Barra,
  menu e painel com `ToggleOmnipresent`. Encaixe: borda = metade/maximizar com prévia; arrastar com modificador =
  regiões (metades e quartos); menu `kalango-layouts` (botão direito no maximizar ou Super+Z) no
  `~/.config/labwc/menu.xml` que o shell grava no idioma (`labwc::sync_layouts_menu`); o labwc lê um só menu.xml.
- **Vários monitores (2026-11-14):** `src/shell/outputs.rs` (área de trabalho, a cada 2 s pelo `wlr-randr`): barra e dock no
  monitor **principal** (`principal=` no `monitores.conf`; sem escolha, o primeiro da esquerda), papel de parede nos
  outros com `kalango-shell --fundo=<saída> --numero=<n>` (app_id `kalango-fundo-<saída>`, uma instância por saída).
  **Para pôr uma janela num monitor use `MoveToOutput output=`** antes do `ToggleFullscreen` (com `MoveTo` o
  fullscreen ia para o monitor em foco). As margens ganham `output=` com mais de um monitor. Mudou o principal → a
  área de trabalho abre outra dela e sai. Configurações → Monitores: desenho do arranjo (arrastar com encaixe nas
  bordas, `displays::place`), Identificar (arquivo `$XDG_RUNTIME_DIR/kalango-identificar` → número por 3 s em cada
  monitor), Tornar principal, Aplicar posição (`--pos`, confirmação de 15 s, `posicao=x,y` no monitores.conf).
  O `monitores.conf` é gravado **ao aplicar**. Posição e principal não têm contagem (não deixam a tela preta);
  resolução/escala/rotação têm, e sem "Manter" volta **só a linha daquele monitor** (`displays::restore_line`;
  regravar o arquivo inteiro apagava o principal e as posições feitos durante a contagem). A área de trabalho
  reaplica o arquivo quando um monitor aparece (cabo ligado de novo). Menu de apps, painel rápido e menu da dock
  têm `MoveToOutput` = monitor principal e já nascem no tamanho dele (`outputs::primary_size`): sem isso a primeira
  imagem saía no meio e pulava para o canto. O painel rápido sabe da placa Wi-Fi pelo `/sys` (rfkill para o
  rádio) antes do `nmcli` responder. O menu do botão
  direito também funciona nas janelas `--fundo` (`DesktopMenuArea`).
- **Dock em janela própria (2026-11-14):** com "sempre visível" e placa de vídeo de verdade
  (`dock::hardware_gl`: i915/xe/amdgpu/radeon/nouveau/nvidia), a área de trabalho abre
  `kalango-shell --dock --faixa=LxA` (app_id `kalango-dock`): sempre por cima das janelas, em todas as áreas, sem
  moldura, encostada na borda. **O labwc não respeita o tamanho pedido pela janela** (dava 1366×683): a regra
  `kalango-dock` do rc.xml tem `ResizeTo` + `MoveTo`, que o shell troca no `~/.config/labwc/rc.xml`
  (`labwc::set_dock_geometry`, junto com as margens) antes de abrir; mudou tamanho/apps/borda → abre outra. A janela
  é transparente (cantos arredondados mostram o que está atrás) e `no-frame` (AppWindow no modo dock: fundo
  transparente, tamanho da faixa). Transparência do fundo da dock: `Theme.dock-transparency` (`dock-transparencia`). **Modelos da dock (2026-11-15):** ícones
  `Theme.dock-icon-style` (plano/3d/ladrilho, `dock-icones`), `dock-icon-transparency` (`dock-icones-transparencia`,
  opaco com o mouse em cima), cantos `dock-corners` (retos/arredondados/pilula, `dock-cantos`) e 5 modelos prontos na
  Barra de tarefas (`DockModelCard`). **No renderer de software, `drop-shadow` e gradiente ignoram o `border-radius`**
  (saem quadrados): o 3D usa sombra/reflexo como retângulos sólidos. A dock em janela própria fica sem sombra
  (`Dock.shadow: false`): a janela tem o tamanho exato e a sombra cortada aparecia nos cantos ("sobra"). Sem placa de vídeo ou com a
  dock escondida, ela continua desenhada na área de trabalho (atrás das janelas). Com placa de vídeo, todo o shell
  usa OpenGL (`renderer_for`: a Aparência travava no renderer de software).
- **Notas da versão nas Atualizações (2026-11-18):** `distro/pacotes/novidades` ([pt] e [en], uma linha curta por
  novidade; **trocar a cada versão publicada**) → o `gerar-deb.sh` põe na descrição do pacote (parágrafos
  "Novidades:"/"What's new:", linhas " - "); a página Atualizações lê pelo `apt-cache show pacote=versão` antes de
  instalar (`updates::parse_notes`, só pacotes `kalango*`) e mostra embaixo da linha ("Interface do KalangOS").
- **Teclas de mídia (2026-11-18):** `XF86Audio*`, `XF86MonBrightness*`, Calculadora, Explorer, WWW, Search e Tools
  no rc.xml → `/usr/local/bin/kalango-midia` (wpctl, brightnessctl, playerctl; valor no centro da barra pelo
  `kalango-aviso`). Volume/brilho/player com `allowWhenLocked`. ISO antiga recebe pelo rc.xml do usuário
  (`labwc::ensure_capture`, bloco `MEDIA_KEYS`). Pacote depende de playerctl e brightnessctl. Segurando a tecla o
  labwc repete o atalho: o script usa `flock -n` (aperto com outro rodando é descartado, sem fila) e a área de
  trabalho lê o `kalango-aviso` a cada 100 ms (antes 1 s: o valor parecia "travado").
- **Tela cheia sem dock (2026-11-18):** `toplevels` lê o estado 3 (tela cheia); app em foco em tela cheia →
  `Shell.app-fullscreen` → `dock::install` fecha a dock em janela própria/faixa (sem mexer no rc.xml) e reabre depois.
- **Ícones em pixel inteiro:** `AppIcon` arredonda tamanho e posição (frações deformavam a borda dos ícones).
- **Firewall ligado por padrão (2026-11-18, pedido do Tiago):** ufw (dependência do pacote), bloqueia o que chega e
  libera o que sai; sempre liberado mDNS 5353/udp. `/usr/local/sbin/kalango-firewall iniciar|ligar|desligar|permitir
  PORTA/PROTO NOME|remover PORTA/PROTO` (pkexec `org.kalango.firewall`), estado em `/etc/kalango/firewall.conf` (todos
  leem). `kalango-firewall.service` roda `iniciar` uma vez por máquina (marca `/var/lib/kalango/firewall-iniciado`):
  antes de ligar, libera o que já escuta fora do loopback (`ss -ltnp`; SSH do HP continua). Compartilhar libera/fecha
  as portas do Samba e wsdd2. A área de trabalho (`src/shell/firewall.rs`) vê porta TCP nova em escuta (< 32768, duas
  voltas de 5 s) bloqueada → zenity "Permitir / Manter bloqueado" (`~/.config/kalango/firewall-bloqueados`).
  **Página própria Configurações → Firewall (2026-11-18, página 13, depois de Compartilhamento;
  `FirewallPage`, `src/settings/firewall.rs`):** liga/desliga, regras com chave ativar/desativar (desativada sai do
  ufw e fica na lista) e Remover, formulário "Abrir portas" (nome, porta ou faixa `6000-6010`, TCP/UDP/os dois,
  rede local ou qualquer um, atalhos SSH/Web/Minecraft/KDE Connect). Arquivo: `permitir=PORTAS/PROTO ORIGEM ESTADO
  NOME` (`qualquer|local`, `on|off`; linhas antigas valem como `qualquer on`); "ambos" vira duas regras; rede local =
  faixas privadas IPv4/IPv6 (uma regra do ufw por faixa). O Samba do Compartilhamento abre só para a rede local.
  Regra desativada também conta como decisão (o aviso não pergunta de novo: `Config::covers_tcp`).
- **Ícone "Instalar KalangOS" na área de trabalho da ISO live (2026-11-18):** `Shell.live`
  (`/run/live/medium`), `ui/components/install_icon.slint`, canto de cima; além do item nos recentes do menu.
- **Menu de apps como o Launchpad (2026-11-18):** uma lista só com rolagem (FIXADOS, RECENTES, TODOS OS APPS =
  `Apps.others`, os instalados sem os fixados e os repetidos); sem o link "Todos os apps". Ícones do menu e da dock
  pelo mesmo `ui/components/app_icon.slint` (`AppIcon`: estilo e cantos da dock).
- **Dock como no Ubuntu (2026-11-16):** "Solta da borda" (`Theme.dock-detached`, `dock-solta=`; só no estilo
  dock) afasta a dock `Theme.dock-gap` = 8 px da borda (`appearance::DOCK_GAP`), com os 4 cantos arredondados; a
  margem do labwc soma a folga e a janela da dock (`dock::geometry`) sai da borda. Botão do menu de programas
  (grade de 9 pontos, id `apps-menu` → `toggle-launcher`): `Theme.dock-apps-button` 0 começo / 1 fim (padrão) /
  2 escondido (`dock-botao-apps=inicio|fim|oculto`); conta como mais um passo no comprimento da dock (Slint e Rust).
- **Dock escondida por cima das janelas (2026-11-16), como no Ubuntu:** com "Sempre visível" desligado e placa de vídeo,
  a área de trabalho abre `--dock-gatilho --faixa=Lx2 --dock-faixa=LxA` (app_id `kalango-dock-gatilho`, faixa
  transparente de `dock::TRIGGER` = 2 px na borda, sempre por cima; regra no rc.xml, que o shell acrescenta se faltar:
  `labwc::ensure_trigger_rule`). O mouse encosta → `Shell.reveal-dock` abre `--dock --faixa=… --ocultar`; essa dock
  fecha sozinha 0,9 s depois de o mouse sair (`dock::install_autohide`, `Shell.dock-hovered` pela TouchArea em volta
  da dock). A posição da dock fica gravada no rc.xml mesmo escondida (é onde a faixa a abre). Margem = 0.
- **Atualização não muda o visual (2026-11-17, pedido do Tiago):** opção nova que muda a aparência entra em
  `appearance::LEGACY` com o valor do comportamento antigo; vale para quem já tem `aparencia.conf` sem a chave (só
  instalação nova recebe o padrão novo). Ex.: `dock-botao-apps` → `oculto`.
- **Integridade da interface (2026-11-17, "é software livre, mas mudar tem de ser claro e com root"):**
  `src/shell/integrity.rs`: ao entrar, a área de trabalho compara os arquivos do pacote `kalango-shell` com o
  `md5sums` do dpkg (MD5 próprio de `tools::codec`); mudou fora do apt e não foi aceito → aviso do zenity com a
  lista, "Aceitar as mudanças" ou "Restaurar o original" pelo `pkexec /usr/local/sbin/kalango-integridade
  aceitar|restaurar` (`org.kalango.integridade`, `auth_admin` sem lembrar). Aceitos em
  `/var/lib/kalango/integridade/aceitos`. **Copiar o binário à mão (HP por SSH) dispara o aviso: aceitar lá.**
  O pacote agora leva as políticas `org.kalango.*` e o `TimeoutStopSec=5s` do `xdg-document-portal` (reiniciar
  ficava 90 s parado esperando ele).
- **Rede na barra e no painel (2026-11-17), como no Ubuntu:** ícone pela rota padrão de menor métrica entre placas
  físicas (`/proc/net/route`, sinal de `/proc/net/wireless`; `system::net_icon`, `Shell.net-icon`): cabo, Wi-Fi com
  1–3 barras, Wi-Fi sem rede/desligado, sem rede. Sem placa Wi-Fi nunca mostra Wi-Fi; placa `unmanaged` (sem
  driver/gerência, veth do Docker) é ignorada. Painel rápido: blocos em sequência, dois por linha
  (`slot()` em `quick_settings.slint`): Cabeada (liga/desliga a placa, lista das conexões de cabo), Wi-Fi, VPN
  (lista; `Device.connection-set`), Bluetooth, Estilo escuro.
- **Captura de tela (2026-11-17), como no Ubuntu:** `src/shell/capture.rs` + `ui/screens/capture.slint` (global
  `Capture`). Print = `kalango-shell --captura` (ferramenta em tela cheia transparente, regra `kalango-captura`:
  área arrastando ou tela inteira, foto ou vídeo); Shift+Print = `--captura=tela` (foto na hora, sem janela);
  Ctrl+Shift+Alt+R = `--captura=video`. Foto pelo `grim` (Imagens/Capturas de tela + `wl-copy`), vídeo pelo
  `wf-recorder` (Vídeos/Gravações de tela) num processo próprio anotado em `$XDG_RUNTIME_DIR/kalango-gravacao`;
  com gravação rodando, qualquer atalho a para (SIGINT). Avisos vão para a barra pelo arquivo
  `$XDG_RUNTIME_DIR/kalango-aviso` (a área de trabalho lê a cada 1 s; `Shell.recording` = "Gravando a tela").
  Sistemas de ISO antiga recebem atalhos e regra pelo `rc.xml` do usuário (`labwc::ensure_capture`); o pacote
  depende de grim, wf-recorder e wl-clipboard.
- **Thunar "Abrir terminal aqui":** o padrão do Debian usa `exo-open --launch TerminalEmulator`, que no Xfce 4.20
  precisa do `xfce4-helpers` (não instalado) e falha calado; `shell::thunar` troca por `foot -D %f` no `uca.xml`.
- **Proteção contra cascata (2026-11-17):** o `examples/snapshot`, rodado dentro da sessão (herdava
  `KALANGO_SESSION=1`), se achava a área de trabalho e abria a dock com o próprio executável → dezenas de processos,
  carga 20, artefatos de vídeo e travamento. Agora `session::is_native` exige também o binário do shell
  (`is_shell_binary`: `kalango-shell`/`kalango_os`), a área de trabalho é única por sessão (`flock` em
  `$XDG_RUNTIME_DIR/kalango-shell-area-de-trabalho.lock`, espera 5 s pela troca de monitor principal) e o snapshot
  apaga as variáveis da sessão e sai se receber `--opção` do shell.
- **Atualizar sem reiniciar e voltar sozinho (2026-11-17):** o autostart do labwc roda o supervisor
  `/usr/local/bin/kalango-shell-sessao` (o postinst troca a linha nos sistemas já instalados): reabre a área de
  trabalho se cair; código 75 = recarregar (atualização ou monitor principal trocado: `update::RELOAD_CODE`);
  3 quedas de menos de 30 s → versão anterior que o **preinst** guarda em `/var/lib/kalango/shell-anterior`
  (`KALANGO_SHELL_ANTERIOR=1` → aviso), e só volta à instalada quando ela muda; para quando o labwc sai. A área de
  trabalho vê o próprio binário trocado (`/proc/self/exe` "(deleted)", `src/shell/update.rs`) e pergunta
  "Recarregar agora" (zenity): fecha o Alt+Tab antigo e sai com 75 (sem supervisor, `exec`); dock/fundos novos
  fecham os antigos. O kalango-shell saiu de `updates::needs_reboot` e o postinst não grava mais
  `/run/reboot-required`. Testar o supervisor: `KALANGO_SESSAO_ATUAL=`/`KALANGO_SESSAO_ANTERIOR=` com scripts falsos.
  No HP: matar a área de trabalho basta (o supervisor reabre com o binário novo).
- **XML do labwc: nunca `--` dentro de `<!-- -->`.** O comentário "kalango-shell --dock-gatilho" deixava o
  `rc.xml` inválido e o labwc perdia as regras seguintes (dock escondida abria no meio, atrás das janelas). Teste
  `labwc::rc_xml_gerado_e_bem_formado`. Trocar dock ↔ faixa fecha o processo antigo na hora (antes ficava aberto).
- **Captura de tela sem grim:** `src` de um capturador wlr-screencopy mínimo no scratchpad da sessão de
  2026-11-17 (wayland-client + png do projeto); compilar de novo se precisar.
- **Compilar nesta máquina KalangOS (desktop do Tiago, RX 570, 2560×1080, só cabo):** Rust em `~/.cargo` e os
  `-dev` que faltam extraídos sem root em `~/.local/kalango-build` (`. ~/.local/kalango-build/env.sh; cargo build
  --profile vm`). O binário daqui roda no KalangOS. Teste `launcher::apps::programas_dos_fixados` falha onde não há
  `htop`.
- **Senhas e PIN (2026-11-16):** `/usr/local/lib/kalango/pam-kalango.sh ligar` (hook `0170` + postinst do pacote; prerm
  desfaz): desvia `/usr/share/pam-configs/unix` para `/usr/share/kalango/pam-unix.debian` (**fora** da pasta: o
  pam-auth-update lê todo arquivo de lá) e gera o nosso sem `obscure`, com `minlen=1` (o padrão do pam_unix é 6) e
  `try_first_pass` no Auth-Initial; Calamares com `allowWeakPasswords`/`allowWeakPasswordsDefault: true`. **PIN**
  (Windows Hello): `/usr/local/sbin/kalango-pin` (sh + perl `crypt` SHA-512, sem dependência nova), hash em
  `/var/lib/kalango/pin/<conta>` (0600 da pessoa, pasta 1733), linha `pam_exec ... expose_authtok ... verificar` antes do
  `login` em greetd/gtklock/swaylock (sudo/su/pkexec não aceitam PIN); 5 erros bloqueiam até entrar com a senha
  (`senha-ok` depois do login). Configurações → Usuários: Definir/Alterar/Remover PIN (senha conferida pelo
  `unix_chkpwd`, que exige `\0` no fim); login mostra "Senha ou PIN" (`Shell.login-has-pin`). Testado com pamtester.
  Testar o `passwd` alimentado por pipe: `script -qec passwd` com `sleep` entre as respostas e `LC_ALL=C`.
- **Revisão de segurança (2026-11-17):** corrigidos (0.1.1-6) o contador do PIN, que seguia links como root na tela
  de login (conta padrão virava dona do `/etc/shadow`; agora `O_NOFOLLOW` + `fchown`, e só vale contador da própria
  pessoa), e pastas com caractere de controle no `kalango-compartilhar` (injetavam config no Samba). Revisados sem
  problema: regra do polkit, scripts de usuários/impressoras/energia/swap/atualizar/integridade, ISO (sem segredos; o
  `user` live não vai para o instalado), capturas do site. **Pendente:** senha do Wi-Fi/VPN e chave privada do WireGuard
  vão como argumento (`ps` de outra conta vê por ~1 s; testar a troca por stdin/arquivo na VM com hwsim); chave do apt
  sem senha (`gpg --passwd`); `gitleaks` no histórico antes de abrir o GitHub; nota de privacidade (logs do nginx);
  aviso de senha de 1 caractere. A verificação de integridade pega mudança fora do apt, não
  um invasor com root (ele também troca o md5sums). Regra: texto vindo da pessoa que vira arquivo de config de root
  passa por validação de caracteres de controle; arquivo gravado por root em pasta gravável por outros nunca segue link.
- **Swap (2026-11-17, pedido do Tiago: sem swap ou a pessoa escolhe o tamanho):** instalador sem partição swap
  por padrão (`partition.conf`: `initialSwapChoice: none`; ainda dá para escolher pequena ou hibernar) e o
  `shellprocess@kalango` roda `/usr/local/lib/kalango/sem-hibernar.sh`, que tira o `resume=` do GRUB quando a swap
  não cabe a RAM (o Calamares o põe sempre; sem a swap, o boot esperava por ela). Depois de instalado:
  Configurações → Energia → **Memória de troca** = arquivo `/swapfile` (0/2/4/8/16/32 GB, como no Ubuntu) pelo
  `pkexec /usr/local/sbin/kalango-swap N` (`org.kalango.swap`; prioridade 10, abaixo da zram de 100; btrfs com
  `mkswapfile`; código 2 = sem espaço); `power::parse_swaps` lê `/proc/swaps` e mostra zram e partição antiga.
  Particionamento manual em UEFI: a FAT32 tem de ir em `/boot/efi` com a flag boot, senão não há GRUB.
- **Calamares 3.3 (trixie) lê o shell só de `user:` → `shell:`**; a chave antiga `userShell` é ignorada (o hook
  `0140` grava as duas). Antes disso a instalação limpa criava o usuário com bash, sem o prompt do Kali.
- **Apps abertos na dock e Alt+Tab (2026-11-14):** `src/shell/toplevels.rs` (wlr-foreign-toplevel, conexão própria numa
  thread, ordem de uso) → `src/shell/taskbar.rs`: janela → id da dock (catálogo, `desktop:<caminho>` pelo nome do arquivo/
  `StartupWMClass`/programa — VS Code = `com.microsoft.VSCode.desktop` —, ou `window:<app_id>`); `Shell.running-ids`,
  `active-app`, `running-extra` (abertos não fixados, depois dos fixados). Clique: abre / foca / minimiza / próxima
  janela; o "em foco" é a última janela de app (clicar na dock tira o foco do app). Botão direito = processo
  `--menu-dock=<id> --pos= --total=` (tela cheia transparente, `DockMenuView`); fixar também pelo botão direito no
  menu de apps (`Shell.save-prefs` grava na hora). Dock nova só mata a antiga 0,9 s depois (sem piscar).
  **Alt+Tab** (`src/shell/switcher.rs`): A-Tab/W-Tab no rc.xml rodam `kalango-shell --alternar[=anterior]`, que só manda o
  pedido pelo socket `$XDG_RUNTIME_DIR/kalango-alternar.sock` ao processo `--alternar=iniciar` (aberto pela área de
  trabalho, sem janela). **No Wayland o winit não mapeia de novo uma janela escondida** (`show()` depois de `hide()`
  ou numa janela criada escondida não faz nada): cada Alt+Tab cria um `AppWindow` novo e o descarta ao terminar. Soltar o Alt = evento cru do winit (feature `unstable-winit-030`; `ModifiersChanged` sem Alt
  ao ganhar o foco cobre o Alt+Tab rápido); painel só aparece após 120 ms.
- **Usuários (2026-11-14), como no Ubuntu:** `src/settings/users.rs` + global `Accounts` (`UsersPage`). A própria senha
  pelo `passwd` num terminal falso (`script -qec passwd /dev/null`, `LC_ALL=C`), respondendo a cada pergunta 250 ms
  depois de ela aparecer (o PAM limpa a entrada ao desligar o eco). Outras contas pelo
  `pkexec /usr/local/sbin/kalango-usuarios criar|senha|tipo|nome|remover` (política `org.kalango.usuarios`, senha
  pela entrada padrão): conta nova com pasta 700 e grupos de desktop (+`sudo` se administrador); recusa remover/
  rebaixar a si mesmo, o último administrador e quem está com sessão aberta. A tela de login mostra todas as contas
  (`Shell.login-users`). Testar o pkexec com as Configurações abertas pela sessão gráfica (pela SSH não há agente).
- **Trocar de usuário sem fechar a sessão (2026-11-14), como no Ubuntu:** `kalango-trocar-usuario` (tela de
  bloqueio, painel rápido) bloqueia e abre `kalango-trocar@N.service` (greetd extra, `/etc/greetd/trocar.toml`,
  `--vt N`, N = 8 a 12; reusa um VT com tela de login sem ninguém). Polkit `50-kalango-trocar.rules`: sessão local
  ativa inicia essas unidades sem senha; `_greetd` pode `lock-session` e `activate`. A tela de login marca quem tem
  sessão aberta (arquivos `/run/systemd/sessions/<id>`, `SERVICE=greetd`: o tipo vem `tty`; só relê se o VT dela
  está na frente) e troca a senha por "Voltar à
  sessão" (bloqueia e ativa; a senha é pedida na tela de bloqueio dela). Botão na tela de bloqueio: módulo
  `gtklock-powerbar-module` (não está no Debian 13), compilado pelo `build.sh` com `distro/gtklock/powerbar.sh`
  (versão + SHA-256 fixos; o texto vem de `KALANGO_TROCAR_USUARIO`). A saída do greeter vai para o VT dele
  (`/dev/vcsN` mostra). Teste por QMP: mover o ponteiro antes de clicar numa janela recém-aberta.
- **Impressoras (2026-11-15):** Configurações → Impressoras (`src/settings/printers.rs`, global `Printers`,
  página 12, mostrada depois de Usuários: a barra lateral segue `order` em `settings.slint`, sem renumerar as
  páginas). CUPS + `cups-browsed` + `ipp-usb`: impressoras de rede e USB modernas aparecem sozinhas (IPP
  Everywhere, sem driver). Lista por `lpstat` (`LC_ALL=C`), relida a cada 3 s com a página aberta; tornar padrão
  (`lpoptions -d`, por pessoa), página de teste, cancelar fila, remover. Adicionar: `lpinfo -l -v` (IPP/dnssd →
  `-m everywhere`; USB antiga → primeiro driver de `lpinfo -m --device-id`) ou pelo endereço
  (`ipp://<ip>/ipp/print`). Quem está no `lpadmin` (administradores; contas padrão não ganham mais) fala direto
  com o CUPS; os outros pelo `pkexec /usr/local/sbin/kalango-impressoras` (`org.kalango.impressoras`).
  "Opções avançadas" = `system-config-printer`. Drivers: hplip, escpr (Epson), brlaser, gutenprint. Teste:
  `ippeveprinter` (cups-ipp-utils) simula uma impressora IPP na rede.
- **Novidades de 2026-11-15:** calendário ao clicar no relógio (`--calendario`, `src/shell/calendar.rs`, regra
  `kalango-calendario`); Atualizações sem terminal (`src/settings/updates.rs`, `pkexec kalango-atualizar
  procurar|instalar`, progresso pelo `APT::Status-Fd`, selo na barra lateral); botões das janelas por estilo
  (`src/settings/window_buttons.rs`: SVGs em `~/.local/share/themes/Kalango-Janelas/openbox-3/` + `<name>` e
  `<titlebar><layout>` no rc.xml do usuário; `botoes-janela=` no aparencia.conf); "Definir como papel de parede" no
  Thunar (`src/shell/thunar.rs` grava a ação no `uca.xml`; o plugin do próprio Thunar sai pelo hook `0150`).
  **Sessões sempre pelo logind** (`LIBSEAT_BACKEND=logind` no kalango-session e no kalango-greeter): com o seatd
  instalado, quem está no grupo video ia para o seatd e a sessão aberta por "Trocar de usuário" caía. Saída do labwc
  no journal (`journalctl -t labwc`).
- **Energia (2026-11-14):** `src/settings/power.rs` + global `Power`: modo (`powerprofilesctl`), apagar a tela/bloquear/
  suspender na bateria e na tomada (`~/.config/kalango/energia.conf`; a área de trabalho roda o `swayidle` com
  `wlopm` e troca ao mudar a escolha ou o carregador; pidfile `kalango-swayidle.pid`), tampa e botão de ligar pelo
  `pkexec /usr/local/sbin/kalango-energia` (`/etc/systemd/logind.conf.d/50-kalango.conf` + HUP no logind).
  `kalango-bloquear` sai se já está bloqueado.
- **Tela de bloqueio (2026-11-14): gtklock** pelo `/usr/local/bin/kalango-bloquear` (Super+L, cadeados): layout
  próprio `/etc/kalango/bloqueio/layout.ui` (ids/sinais do gtklock 4; @SENHA@/@DESBLOQUEAR@ trocados no idioma),
  `style.css`, papel de parede da pessoa e módulo userinfo (precisa do `accountsservice`). **Config do gtklock é key
  file: comentário só com `#`** (`;` descarta tudo). **Não usar marcação Pango no `-t`** (derruba o gtklock: tela
  preta presa, o ext-session-lock fica travado). O script roda sem `-d` e reabre se cair; 3 quedas → swaylock.
  Testar bloqueio sempre na VM antes da máquina física.
- **Pendrives (2026-11-13): a cópia não termina antes da gravação.** `etc/udev/rules.d/90-kalango-pendrive.rules`
  (`90-`: precisa do `ID_BUS` do 60-persistent-storage) põe `bdi/strict_limit=1` e `bdi/max_bytes=16 MB` em disco
  USB: a cópia anda na velocidade real. A área de trabalho (`src/shell/usb.rs`, thread a cada 2 s) dá `sync -f` nos
  discos USB montados e mostra "Gravando no pendrive, não remova" no centro da barra (`Shell.usb-writing`; fim =
  aviso pelo `show-toast`, público). O udisks já monta FAT32 com `flush`; exFAT não tem. Medido em exFAT a 2 MB/s
  (`run-qemu.sh --pendrive`), 60 MB: sem nada o `cp` acaba em 0,2 s com 58 MB na memória; com regra + shell, 29 s e
  nada pendente.
- **Menu do botão direito na área de trabalho** (`ui/components/desktop_menu.slint`): papel de parede, dock,
  monitores, terminal, arquivos, Configurações. Desenhado **dentro** da janela (não `PopupWindow`): a área de
  trabalho é tela cheia e sempre atrás, e o labwc não entrega cliques a popups dela. Abre as Configurações
  numa página com `settings:<n>` → `kalango-shell --settings --pagina=<n>`; se já estiverem abertas, o pedido
  vai por `$XDG_RUNTIME_DIR/kalango-settings.pagina` (`session::watch_page_requests`).
- **Loja = GNOME Software + Flatpak/Flathub** (decidido em 2026-11-12): pacotes `flatpak` e
  `gnome-software-plugin-flatpak`; o hook `0110` faz `flatpak remote-add --system flathub` a partir de
  `usr/share/kalango/flathub.flatpakrepo` (arquivo oficial com a chave; sem rede no build). O `kalango-session`
  lê `/etc/profile.d/flatpak.sh` (o greetd não roda shell de login) para os apps Flatpak entrarem no
  `XDG_DATA_DIRS` ("Todos os apps"). Agente do polkit: `lxpolkit` no autostart do labwc (janela de senha). O
  Flatpak do Debian libera instalar para o grupo `sudo` sem senha. Na ISO live, o que se instala fica na RAM
  (overlay = metade da memória): apps grandes podem não caber.
- **Instalador: Calamares** com a sequência do `calamares-settings-debian` e, por cima (`etc/calamares/`): marca
  `branding/kalango` (logo, slides, `stylesheet.qss` escuro), `locale.conf` (Nova York; a pessoa escolhe), `packages.conf`
  (remove também o calamares) e `shellprocess@kalango` (tira o `[initial_session]` do greetd e o
  `kalango-instalar`; **não usar `$` nos comandos**, o Calamares trata `$nome` como variável).
  `kalango-instalar` abre o Calamares com `sudo env WAYLAND_DISPLAY=... calamares` (a sessão não tem agente do
  polkit para o pkexec). O GRUB vai por apt a partir da mídia: `package-lists/kalango.list.binary` põe grub-pc,
  grub-efi e cryptsetup no repositório da ISO. Testado em VM: instala, dá boot pelo disco e entra no greeter.
- **Ícone e GRUB (2026-11-15):** ícone novo = K do logotipo como tronco + calango subindo + rabo de fita de luz
  (`assets/logo/gerar_icone.py`; o Tiago escolheu a versão Noite, `Kalango_Icone_Noite.png`). Tema do GRUB por
  `distro/grub/gerar-tema.py` (precisa de `grub-mkfont`; rodar de novo se mudar fundo/ícone): sistema instalado em
  `/boot/grub/themes/kalango` + `GRUB_THEME` no `grub.d/kalango.cfg` (o 10_linux põe `--class kalango`, a 1ª palavra do
  GRUB_DISTRIBUTOR); ISO em `bootloaders/grub-pc/{grub.cfg,theme.cfg,live-theme/}` (menu próprio com classes e "safe
  graphics" = nomodeset). No tema: seleção só com fatias w/c/e (n/s deslocam o texto), fonte = família + Regular/Bold,
  a `progress_bar` ignora a altura (contagem é um `label` com `id="__timeout__"`).
- **Rádios depois de suspender (2026-11-15):** no HP (Intel Wireless 3160) o `iwlwifi` volta com RF_KILL de hardware e o
  Bluetooth (USB 8087:07dc) some. `/usr/lib/systemd/system-sleep/kalango-radios` guarda o estado antes e, só se quebrou,
  recarrega o driver → remove/rescan do PCI; btusb → religa o hub USB (numa unidade `systemd-run`: dentro do hook
  congelava a sessão ~20 s). Nada disso desfaz o bloqueio da BIOS: se continuar bloqueado depois do deep (S3), grava
  `/etc/systemd/sleep.conf.d/50-kalango-radios.conf` (`MemorySleepMode=s2idle`). Testado: em s2idle o HP volta com
  Wi-Fi e Bluetooth. Log: `journalctl -t kalango-radios`.
- **Apps padrão (2026-11-15):** Evince (PDF), LibreOffice Writer/Calc/Impress (pt-BR; Word/Excel/PowerPoint), File Roller
  + thunar-archive-plugin, 7zip e unar. Dois cliques num compactado **extrai ao lado**, como no macOS:
  `kalango-extrair.desktop` (`file-roller --extract-here`: vários arquivos → pasta com o nome; um só → o arquivo) é o
  padrão em `/etc/xdg/mimeapps.list`. Em "Computador" do Thunar, o disco do sistema aparece como drive que não abre (as
  partições são de sistema; o gvfs 1.57 lista todo drive e ignora `UDISKS_IGNORE` e `ID_SEAT`): o acesso é "Sistema de
  arquivos".
- **Atualizações OTA (2026-11-15):** o shell e os scripts de `/usr/local/{bin,sbin,lib/kalango}` + `kalango-radios` vão no
  pacote **`kalango-shell`** (`distro/pacotes/gerar-deb.sh`, versão `<Cargo.toml>-<KALANGO_REVISAO>`; desde a 0.1.1 leva
  também os temas do terminal e `/usr/share/kalango/zshrc`: o postinst põe o `.zshrc` no `/etc/skel`, deixa o zsh padrão
  para contas novas e, uma vez por máquina (`/var/lib/kalango/terminal-zsh`), passa do bash para o zsh quem já existe;
  depende de zsh e plugins), instalado na ISO por
  `config/packages.chroot` (cópia em `distro/out/`). Repositório apt assinado em `https://kalangos.org/apt` (desde 2026-11-17; o endereço antigo
  `kalangos.easysmartcontroll.com/apt` serve os mesmos arquivos e o postinst migra o `kalango.sources` e o `HOME_URL`)
  (suite trixie, main; `etc/apt/sources.list.d/kalango.sources`, chave `0B89DC19D08F8A6B` em
  `usr/share/keyrings/kalango-archive-keyring.gpg`). **A chave privada fica só na máquina do Tiago**
  (`~/.local/share/kalango-repo/gnupg`; no desktop KalangOS do Tiago em `~/Documents/.local/share/kalango-repo`,
  usar `KALANGO_REPO=`); `distro/repo/publicar.sh` traz o pool do servidor, monta/assina e envia por rsync
  (`KALANGO_REPO_DESTINO`); o VPS só serve estático (`distro/repo/nginx-kalangos.conf`; VPS com aaPanel, acesso em `CLAUDE.local.md`,
  arquivos em `/www/wwwroot/kalangos.easysmartcontroll.com/apt`, certbot webroot; não mexer nos outros sites).
  Publicado `0.1.0-3` em 2026-11-15 (o HP o viu pela internet). Guia: `distro/repo/LEIAME.md`.
  **ISO pública (2026-11-17):** `https://kalangos.org/download/kalango-os-0.1.1.iso` (+ `.sha256`; pasta
  `/www/wwwroot/kalangos.org/download`, que o `publicar.sh` do site não apaga), com o `kalango-shell 0.1.1-5`, o
  mesmo arquivo publicado no apt (a 0.1.1-6, com as correções de segurança, chega na 1ª atualização). **Cada ISO leva uma revisão nova** (`KALANGO_REVISAO=N distro/build.sh`) e o
  `.deb` de `distro/out` é publicado junto: mesma versão com outro conteúdo faz o apt da ISO tentar "voltar" de
  versão e o `lb build` falha no fim (o `build.sh` confere isso no início).
  **Codinomes = plantas do sertão nordestino (2026-11-17, pedido do Tiago), um por série (`distro/codinomes`):
  0.1 = Mandacaru.** O `build.sh` gera `kalango-os-<versão>-<codinome>[-hardware-novo].iso` e põe no os-release
  `PRETTY_NAME="KalangOS 0.1 Mandacaru"`, `VERSION=...` e `KALANGO_CODENAME=`; `VERSION_CODENAME` continua `trixie`
  (scripts de repositórios de terceiros, como Docker e VS Code, leem esse campo). O shell mostra o codinome em Sobre
  o sistema (`device::codename`, `include_str!` da tabela; o `build.sh` copia `distro/codinomes` para o builder).
  **Duas edições (2026-11-17, pedido do Tiago):** a padrão (kernel 6.12 do Debian 13, para máquinas antigas) e a
  **hardware novo** (`KALANGO_EDICAO=novo` → `kalango-os-<versão>-hardware-novo.iso`): `lb config --backports true` +
  `distro/edicoes/hardware-novo.pref` (pin 990 só para `linux-*`, `firmware-*`, `mesa-*` e libs do Mesa; o resto
  fica estável), copiado para `config/archives` (montagem) e `/etc/apt/preferences.d` (o instalado segue nos kernels
  novos). Em 2026-11: kernel 7.2, Mesa 26.1, firmware 2026-09, como o Ubuntu 26.10 (o único que funcionou num
  notebook Core Ultra do Tiago). `KALANGO_DEB=<.deb publicado>` põe o pacote do apt na ISO sem compilar outro (ele
  fica em `~/Documents/.local/share/kalango-repo/www/apt/pool/...`; não use um .deb de `distro/out`, que o build.sh
  apaga antes de copiar). Os scripts de download usam `wget`
  quando não há `curl`. A VPN do servidor (`wg0`, NetworkManager) não liga sozinha: `nmcli connection up wg0`.
  Testado no HP: 0.1.0-1 → 0.1.0-2 pelo apt, com aviso de reiniciar (`/run/reboot-required`). Bugs e dicas: GitHub
  Issues (modelos em `.github/ISSUE_TEMPLATE`) + Discussions (ligar nas configurações do repositório).
- Identidade: `/usr/lib/os-release` = "KalangOS" (`ID=kalango`, `ID_LIKE=debian`, protegido de atualização do
  base-files pelo hook `0050`), `etc/default/grub.d/kalango.cfg` (`GRUB_DISTRIBUTOR`).
- Próximos passos da arquitetura: layer-shell para barra/dock/menu; contraste das legendas de partição do
  Calamares no tema escuro (desenhadas pela paleta Qt, não pelo qss).

## Organização do código (2026-11-14)

- `src/common/` (relógio, idioma, CPU/bateria), `src/launcher/` (menu de apps, `.desktop`, recentes),
  `src/shell/` (sessão e processos do shell: login, dock, barra, Alt+Tab, monitores, papel de parede, labwc, USB) e
  `src/settings/` (o que fica por trás das páginas das Configurações). Caminho completo nos `use`
  (`crate::shell::session::...`); o `lib.rs` só reexporta `Mode`. UI: `ui/screens/` (telas), `ui/components/`
  (peças reutilizáveis), globals em `ui/globals.slint`, `ui/device.slint` e `ui/theme.slint`.

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
- **Teste rápido: `distro/testar-vm.sh`** (sem sudo, sem ISO): compila com o perfil `vm` do Cargo.toml (sem LTO,
  ~12 s incremental) dentro do builder via `bwrap`, monta `target/vm-dev/{kalango-shell,raiz/}` e chama
  `run-qemu.sh --dev` (9p, tag `kalango-dev`). Na ISO live, `kalango-dev.service` (antes do greetd) roda
  `/usr/local/sbin/kalango-dev-aplicar`, que só age com `/run/live/medium` + a tag 9p. `--so-preparar` + na VM
  `sudo kalango-dev-aplicar --reiniciar` itera sem reiniciar a VM; `--tudo` leva papéis de parede e ícones.
- Teste manual antigo: compilar contra o trixie com `bwrap --ro-bind distro/.cache/builder /` +
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
  nome exibido "User" (`user-fullname` no `auto/config`).
  Para depurar, Ctrl+Alt+F2 dá console com login automático (usuário `user`, senha `live`);
  `journalctl -t kalango-shell` tem a saída do shell. O greetd está no VT7.
- Tema escuro dos apps: `etc/gtk-3.0/settings.ini`, `etc/gtk-4.0/settings.ini`, `etc/xdg/labwc/themerc-override`,
  `etc/xdg/foot/foot.ini` (mesmas cores de `ui/theme.slint`); libadwaita (Loja, Agenda) só lê a preferência pelo
  portal, que a sessão não tem: `ADW_DEBUG_COLOR_SCHEME=prefer-dark` no `environment` do labwc (+ override de
  GSettings `90_kalango.gschema.override`, compilado pelo hook `0100`).
- Tema GTK 3 próprio **Kalango** (`/usr/share/themes/Kalango/gtk-3.0/gtk.css`): importa o Adwaita escuro embutido
  e troca as cores pelas do shell; o destaque é a cor nomeada `@kalango_accent`, que o shell redefine em
  `~/.config/gtk-3.0/gtk.css` (com marca; vale para apps abertos depois, e o Thunar fica vivo como daemon:
  `thunar -q`). Ativado em `settings.ini` e no gschema override. Padrões do Thunar (ícones, caminho em botões,
  sem barra de menu) em `etc/xdg/xfce4/xfconf/xfce-perchannel-xml/thunar.xml`; miniaturas pelo `tumbler`.
- Editor de código ("Código" da dock, `session::command_for`): **Geany** com o esquema
  `usr/share/geany/colorschemes/kalango-dark-modern.conf` e padrões globais em `usr/share/geany/geany.conf`
  (números de linha, JetBrains Mono, régua na coluna 100, sem barra de ferramentas; o Geany só usa esse arquivo
  enquanto o usuário não tem `~/.config/geany/geany.conf`). Sem Geany, o "Código" abre o Mousepad. Mousepad usa
  o estilo GtkSourceView `usr/share/gtksourceview-4/styles/kalango.xml` (padrões no gschema override). Os dois:
  fundo/seleção/números do tema do sistema + cores de sintaxe do VS Code Dark Modern.
- Temas de ícones extras (Aparência → Ícones, escolhidos pelo Tiago em 2026-11-13): **Colloid** e **Fluent**
  (GPL-3.0, vinceliuice), instalados por `distro/icones/temas-extras.sh` (commit fixo + SHA-256; ~6 min no build).
  Colloid: base `Colloid-Dark` + `Colloid-Light` (a escura aponta pastas da clara) e `Kalango-Colloid-<Cor>` só
  com os arquivos que mudam; `-a` evita os ícones que imitam App Store/Finder. Fluent: bases ocultas
  `.Fluent-base`/`.Fluent-dark-base` + `Fluent-<cor>[-dark]`, e `Kalango-Fluent-<Cor>` é só um nome (Inherits).
  `Theme.icon-theme` (0 Flat Remix, 1 Colloid, 2 Fluent; `icones=` no aparencia.conf) +
  `appearance::icon_theme_name`; a área de trabalho recarrega os ícones da dock ao trocar.
- Ícones: **Flat Remix** (GPL-3.0, escolhido pelo Tiago em 2026-11-12; não está no Debian). `distro/icones/
  flat-remix.sh` (chamado pelo `build.sh`) baixa a versão fixa (tag + SHA-256) para `distro/.cache` e instala
  `Flat-Remix-Blue-Dark` inteiro (~54 MB) + `Kalango-<Cor>` (6, um por destaque: Cyan, Violet, Blue, Green,
  Yellow, Magenta) só com as pastas que diferem da base (~1,3 MB cada), herdando dele. Padrão `Kalango-Ciano`
  (settings.ini + gschema); o shell troca a variante com `gsettings` ao mudar o destaque (só se o tema atual for
  um Kalango) e os apps abertos mudam na hora. Índices pelo hook `0100`. A dock, o menu e "Todos os apps"
  usam os ícones do tema na sessão real (`desktop::find_themed_icon` segue o `Inherits` como o GTK;
  `apps::themed_icon_names`); `tint` transparente = ícone colorido próprio. Descartados: Papirus (187 MB),
  WhiteSur e La Capitaine (imitam o Finder/App Store da Apple).
- **Secure Boot (2026-11-13: funcionar ligado ou desligado, quem usa decide):** a ISO já liga pelo shim assinado do
  Debian; `shim-signed` + `grub-efi-amd64-signed` vão no repositório da ISO (`kalango.list.binary`) e o hook `0130`
  faz o helper do Calamares instalá-los (o grub-install do Debian põe o shim na partição EFI).
  `run-qemu.sh --secureboot` (OVMF com chaves da Microsoft; variáveis em `out/ovmf-vars-secureboot.fd`).
- `run-qemu.sh --disco` liga um disco de 20 GB (`distro/out/disco-teste.qcow2`) para testar o instalador;
  `--so-disco` dá boot pelo disco sem a ISO.
- **`/tmp` no disco (2026-11-14), como no Ubuntu:** o Debian 13 põe o `/tmp` em memória (metade da RAM) e o
  ESP-IDF falhava com "No space left on device". `etc/systemd/system/tmp.mount` → `/dev/null` (mascarado) e
  `etc/calamares/modules/fstab.conf` (o do Debian + `tmpOptions` com `tmpfs: false`, senão o Calamares grava
  `tmpfs /tmp` no fstab).
- `auto/config` usa `noeject` (desligar não para pedindo para remover a mídia).
- `distro/auto/config`: trixie amd64, main+contrib+non-free-firmware, en_US (+ pt_BR), teclado us, usuário live `user`.
- Pacotes em `distro/config/package-lists/kalango.list.chroot`; configs de sistema em `distro/config/includes.chroot/`.

## Assets (já organizados)

- `assets/layout/` — telas de referência 2880×1800: `login.png`, `desktop.png`, `menu_apps.png`, `config_aparencia.png`, `logotipo.png`.
- `assets/wallpapers/16x9/` — 7 wallpapers (Preto, Branco, Azul, Verde, Roxo, Laranja, PretoAzul) em 3840×2160,
  2560×1440 e 1920×1080, gerados por `assets/wallpapers/gerar.py` (numpy + Pillow; `--previa` = só Full HD):
  fitas de luz translúcidas, hexágonos nos cantos e o logotipo real (letras azul-marinho no Branco).
- `assets/wallpapers/` — 6 wallpapers 2880×1800: Ciano, Roxo, Azul, Verde, Amarelo, Rosa
  (mesma ordem das 6 cores de destaque da tela Aparência → trocar a cor de destaque pode trocar o wallpaper).
- `assets/logo/` — `Kalango_Logo_Vetorizado.svg` e `Kalango_Logo_transparente.png` (2172×724).
- `ui/icons/` — 32 ícones SVG de traço (24×24, stroke branco; aplicar cor com `colorize`):
  folder, terminal, globe, code, bag, settings, image, music, file-text, file, calendar, activity, download,
  search, lock, power, wifi, volume, battery, x, chevron-left/right, arrow-right, user, accessibility,
  monitor, bluetooth, info, contrast, dock, chevron-down, grid (9 pontos, preenchido).
- Na raiz ainda estão os arquivos originais enviados (zips, logo .svg/.png duplicados). Não apagar sem perguntar ao Tiago.

## Especificação extraída do layout

Escala: layout em 2880×1800 físico; janela de referência **1440×900 lógico** (medidas abaixo já em px lógicos).

**Cores (tema Escuro/vidro):** fundo `#07090d`, painel/janela `#0d1218`, tile `#131920`, borda `#1e2a33`,
texto `#e6edf3`, texto secundário `#8b98a5`, perigo (power) `#f07178`, linhas de circuito = destaque com ~35% alfa,
grade de fundo sutil (~48px).
**Cores de destaque (ordem):** ciano `#3dd6f5`, roxo `#a78bfa`, azul `#7ea6f7`, verde `#5fd38d`, âmbar `#e8b04b`, rosa `#f28b95`.
**Fontes:** sans tipo Inter para UI; mono tipo JetBrains Mono para relógio, barra, rótulos de seção (MAIÚSCULAS espaçadas) e terminal.
**Marca:** usar os arquivos reais da marca em `ui/brand/` (global `Brand` em `ui/brand.slint`), nunca desenho próprio.
Ícone (barra, menu, janela, Calamares): `kalango-icon.png` = o K com o calango (R1, `assets/logo/Kalango_Icone_Noite.png`;
  o "g"-calango antigo saiu em 2026-11-15).
Logotipo: `kalango-logo.png` (letras brancas) e `kalango-logo-claro.png` (letras azul-marinho, tema claro).
A pílula da barra (abre o menu) é o ícone do K seguido do texto "alango" (lê-se "Kalango"); **não** o logotipo do
papel de parede (o Tiago corrigiu em 2026-11-15).
O SVG de `assets/logo` é rastreado de bitmap e fica granulado em tamanho pequeno; só trocar para vetor se vier um desenhado à mão.

1. **Tela de login:** relógio grande mono (~108px) "19:38" + "quarta-feira, 11 de novembro" no topo; card central
   ~380×320 (avatar círculo com "U" e borda destaque, "Usuário", campo "Senha" com botão seta em destaque,
   link "Outro usuário", rótulo "wayland"); hexágono e linhas de circuito no fundo; rodapé: marca + "KalangOS"
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
