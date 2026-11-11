# Kalango OS — interface (shell) em Rust + Slint

Projeto da interface gráfica do **Kalango OS**, distro Linux proprietária.
Comentários, docs e commits seguem as preferências globais do Tiago (pt-BR, Conventional Commits, JSDoc/rustdoc em tudo que é público).

## Decisões já tomadas

- **Base do sistema:** Debian 13 "trixie" (kernel Linux + userland Debian). ISO a ser gerada com `live-build`.
- **Linguagem/UI:** Rust + Slint (resolvido para **slint 1.18.1** no Cargo.lock).
  - Dev: janela comum no macOS (backend winit, renderer femtovg).
  - Dispositivo: `cargo build --release --features kms` (desenha direto via DRM/KMS).
- **Ponto em aberto (arquitetura):** o layout tem janelas de apps reais (Arquivos, Terminal), então o desktop final
  precisa de um **compositor Wayland**. Opções: (a) compositor pronto (labwc/wayfire) + shell em Slint;
  (b) compositor próprio em Rust com Smithay (caminho do COSMIC). Por ora, construir um **protótipo navegável
  de tela cheia** que reproduz o layout (login → área de trabalho → menu de apps → configurações), com as
  janelas de apps como mock dentro do próprio Slint. Decidir o compositor depois de validar o visual.

## Ambiente

- Rust instalado via rustup em `~/.cargo` **sem** alterar o PATH: usar `~/.cargo/bin/cargo` ou rodar `source ~/.cargo/env`.
- O esqueleto atual compila (`cargo build` OK).

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
**Marca (ícone da barra):** "bonequinho" de calango em traço: cabeça losango, braços e pernas angulados, rabo curvo.

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
