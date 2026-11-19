# Mapa do código

Onde fica cada coisa. Cada arquivo `.rs` começa com um comentário `//!` que
explica o que ele faz e as decisões por trás; esta página é o índice.

## Entrada

| Arquivo | Papel |
|---|---|
| `src/main.rs` | Só chama `kalango_os::run()`. |
| `src/lib.rs` | Escolhe o modo pelos argumentos, o renderizador (software ou OpenGL), monta a janela (`build_mode`) ligando todos os módulos, e a moldura própria das Configurações. |
| `build.rs` | Compila `ui/app_window.slint` e embute as traduções de `lang/`. |

## `src/common/`: usado por todos

| Arquivo | Papel |
|---|---|
| `clock.rs` | Relógio da barra e da tela de login, no idioma da interface. |
| `i18n.rs` | Idioma (`tr("en", "pt")`, `lang()`). |
| `system.rs` | CPU, bateria e ícone de rede da barra (lê `/proc` e `/sys`). |

## `src/launcher/`: menu de apps

| Arquivo | Papel |
|---|---|
| `apps.rs` | Catálogo de apps fixados, busca, ícones do tema, "Todos os apps", redução de ícones PNG grandes. |
| `desktop.rs` | Lê os arquivos `.desktop` do sistema (nome traduzido, ícone, comando `Exec`) e acha ícones no tema como o GTK faz. |
| `icon_style.rs` | Estilos dos ícones (Configurações → Barra de tarefas → Ícones). |
| `recent.rs` | Arquivos recentes (`~/.local/share/recently-used.xbel`). |

## `src/shell/`: a sessão

| Arquivo | Papel |
|---|---|
| `session.rs` | Modos do binário, abrir apps (`command_for`), desligar, arquivo de PID, uma área de trabalho por sessão. **Comece por aqui.** |
| `greeter.rs` | Tela de login: conversa com o greetd pelo socket `$GREETD_SOCK`. |
| `labwc.rs` | Gera o `~/.config/labwc/rc.xml` (margens, dock, atalhos, regras) e o `menu.xml`. Só escreve arquivos com a marca "Gerado pelo kalango-shell". |
| `dock.rs` | Dock em janela própria, dock escondida que aparece com o mouse, tamanho e posição. |
| `taskbar.rs` | Apps abertos na dock, qual janela é de qual app, menu do botão direito. |
| `toplevels.rs` | Janelas abertas e em foco (protocolo `wlr-foreign-toplevel`, numa thread com conexão Wayland própria). |
| `switcher.rs` | Alt+Tab. |
| `workspaces.rs` | Áreas de trabalho na barra (`ext-workspace-v1`). |
| `outputs.rs` | Vários monitores: principal, papel de parede nos outros. |
| `wallpaper.rs` | Papéis de parede: modelos (`MODELS`), imagem própria, miniaturas. |
| `desktop_icons.rs` | Ícones da pasta Desktop e Lixeira na área de trabalho. |
| `trash.rs` | Lixeira cheia/vazia e esvaziar. |
| `calendar.rs` | Calendário do relógio. |
| `capture.rs` | Captura e gravação de tela (`grim`, `wf-recorder`). |
| `osd.rs` | Aviso flutuante de volume/brilho: desenha sem winit, numa camada `layer-shell` e memória compartilhada. O código Wayland mais baixo nível do projeto. |
| `usb.rs` | Avisa enquanto grava no pendrive e força a gravação (`sync`). |
| `firewall.rs` | Pergunta quando um programa novo espera conexões e o firewall bloqueia. |
| `integrity.rs` | Compara os arquivos do pacote com o `md5sums` do dpkg e avisa se mudaram fora do apt. |
| `update.rs` | Percebe que o próprio binário foi atualizado e pergunta "Recarregar agora". |
| `thunar.rs` | Ações no botão direito do Thunar ("Definir como papel de parede", "Abrir terminal aqui"). |

## `src/settings/`: as Configurações

Cada página tem um módulo. Quase todos seguem o mesmo padrão: funções `parse_*`
puras e testadas que leem a saída de um comando, e um `install(ui, mode)` que
liga os callbacks e lê o sistema numa thread.

| Arquivo | Página | Fala com |
|---|---|---|
| `appearance.rs` | Aparência, Barra de tarefas | `~/.config/kalango/aparencia.conf`, `gsettings` |
| `device.rs` | Rede e Wi-Fi, Som, Energia (bateria), Sobre | `nmcli`, `wpctl`, `/sys`, `/proc` |
| `netconfig.rs` | Rede → IP fixo, VPN | `nmcli`, `gdbus` (WireGuard) |
| `bluetooth.rs` | Bluetooth | `bluetoothctl` |
| `displays.rs` | Monitores | `wlr-randr`, `monitores.conf` |
| `power.rs` | Energia | `powerprofilesctl`, `swayidle`, `kalango-energia`, `kalango-swap` |
| `users.rs` | Usuários, PIN | `passwd`, `kalango-usuarios`, `kalango-pin` |
| `printers.rs` | Impressoras | `lpstat`, `lpadmin`, `kalango-impressoras` |
| `sharing.rs` | Compartilhamento | Samba via `kalango-compartilhar`, `avahi-browse` |
| `firewall.rs` | Firewall | `kalango-firewall` (ufw) |
| `region.rs` | Idioma e região | `regiao.conf`, labwc |
| `shortcuts.rs` | Atalhos de teclado | `atalhos.conf` → `rc.xml` |
| `updates.rs` | Atualizações | `kalango-atualizar` (apt), Flatpak |
| `terminal.rs` | Aparência → Terminal | temas do foot e do Ptyxis |
| `window_buttons.rs` | Aparência → botões das janelas | tema do labwc |

## `src/tools/`: apps próprios

| Arquivo | Papel |
|---|---|
| `calculator.rs` | Liga a Calculadora à tela. |
| `expr.rs` | Avaliador de expressões (sem `eval`, sem crates). |
| `codec.rs` | Base64, URL, MD5/SHA-1/SHA-256, UUID, JSON, cores, tempo Unix. |

## `ui/`: as telas

| Arquivo | Conteúdo |
|---|---|
| `app_window.slint` | Janela raiz: decide o que mostrar em cada modo. |
| `theme.slint` | Cores, fontes, tamanhos, todas as opções de aparência (global `Theme`). |
| `globals.slint` | Global `Shell` (estado da sessão), `Apps` (catálogo). |
| `device.slint` | Global `Device` e os outros das Configurações (estado real). |
| `icons.slint`, `brand.slint` | Ícones de traço e a marca. |
| `components/` | Peças reutilizáveis: barra, dock, ícone de app, botões, moldura de janela, menu da área de trabalho. |
| `screens/` | Telas inteiras: login, menu, painel rápido, Configurações (`settings_pages.slint` tem ~3.700 linhas, uma página por componente), calculadora, captura, Alt+Tab. |
| `apps/` | Janelas de demonstração do protótipo. |

## `distro/`: o sistema

| Pasta/arquivo | Papel |
|---|---|
| `build.sh` | Gera a ISO (ver [ISO e testes](iso-e-testes.md)). |
| `gerar-isos.sh` | Gera as duas edições com o pacote publicado e envia ao site. |
| `testar-vm.sh`, `run-qemu.sh` | Testes em máquina virtual. |
| `auto/config` | Opções do live-build (Debian trixie, idiomas, usuário live). |
| `config/package-lists/` | Pacotes do Debian que entram na ISO. |
| `config/includes.chroot/` | Arquivos copiados como estão para o sistema (`/etc`, `/usr`…). |
| `config/hooks/live/` | Scripts que rodam dentro do sistema durante a montagem da ISO. |
| `pacotes/` | Geradores dos pacotes `.deb` e as notas da versão (`novidades`). |
| `repo/` | Repositório apt: chave, publicar, nginx. |
| `edicoes/hwe.pref` | Prioridade do apt para a edição HWE. |
| `codinomes` | Codinome de cada série (0.1 = Mandacaru). |
| `grub/`, `icones/`, `gtklock/` | Tema do GRUB, temas de ícones e módulo da tela de bloqueio, baixados com versão e SHA-256 fixos. |
| `migracao/` | Backup e restauração ao reinstalar. |

## `assets/`

Logo, ícone e papéis de parede. Os papéis de parede desenhados são gerados por
`assets/wallpapers/gerar.py` (PNG); os de foto, por `assets/wallpapers/fotos.py`
(JPEG, a partir das fotos originais).

## Testes

```sh
cargo test                  # tudo
cargo test --lib power      # só o que tem "power" no nome
```

Os testes ficam no fim de cada arquivo (`#[cfg(test)] mod tests`). Dois deles
dependem da máquina: `launcher::apps::programas_dos_fixados` falha onde não há
`htop`, e o do terminal aceita Ptyxis ou foot.
