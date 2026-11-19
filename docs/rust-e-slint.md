# Rust e Slint: as linguagens da interface

A interface tem duas partes que se completam:

- **Slint** (`ui/*.slint`): descreve **o que aparece** na tela (botões, listas,
  cores, animações). É uma linguagem declarativa, parecida com QML ou CSS com
  lógica.
- **Rust** (`src/**/*.rs`): faz **o trabalho**: lê o sistema, roda comandos,
  grava arquivos e responde aos cliques.

O `build.rs` compila os `.slint` para código Rust no `cargo build`. Um erro de
digitação numa tela aparece na compilação, não com o sistema rodando.

## Por que Rust

- **Segurança de memória sem coletor de lixo.** As falhas mais comuns em
  programas de sistema (estouro de buffer, uso de memória já liberada) não
  compilam em Rust, a menos que se use `unsafe`. **Este projeto não tem nenhum
  bloco `unsafe`**: mantenha assim.
- Binário único, rápido e pequeno, sem máquina virtual nem interpretador.
- Erros são valores (`Result`, `Option`): o compilador obriga a tratar o caso de
  falha.

## Instalação e comandos

O Rust fica em `~/.cargo` (instalado pelo `rustup`).

```sh
source ~/.cargo/env                # põe o cargo no PATH desta janela
cargo build                        # compila (perfil de desenvolvimento)
cargo run                          # roda o protótipo numa janela
cargo test                         # roda os testes (≈165)
cargo clippy --all-targets         # análise de estilo e erros comuns (o CI exige zero avisos)
cargo build --profile vm           # rápido, otimizado, sem LTO: para testar no KalangOS
cargo build --release --locked     # o binário que vai no pacote (≈7 min, com LTO)
```

Perfis (em `Cargo.toml`):

| Perfil | Uso | Detalhe |
|---|---|---|
| `dev` (padrão) | desenvolver no computador | rápido de compilar, lento de rodar |
| `vm` | testar na VM ou no KalangOS | otimizado, sem LTO, incremental (~12 s) |
| `release` | pacote publicado | LTO e `codegen-units = 1`: menor e mais rápido, compila em ~7 min |

`--locked` faz o cargo usar **exatamente** as versões do `Cargo.lock`. Use
sempre nos builds que viram pacote: sem ele uma dependência poderia mudar sem
você ver.

**Compilar para a ISO:** a biblioteca C do Ubuntu (glibc 2.44) é mais nova que a
do Debian 13 (2.41). Um binário compilado num Ubuntu **não roda** no KalangOS.
Compile dentro do KalangOS/Debian 13 ou deixe o `distro/build.sh` compilar no
chroot dele.

## Dependências (crates)

As bibliotecas diretas são poucas, de propósito:

| Crate | Para quê |
|---|---|
| `slint` | a biblioteca de interface |
| `image` | ler/gravar PNG, JPEG e WebP (papel de parede, ícones) |
| `resvg` | desenhar SVG |
| `chrono` | data e hora local |
| `wayland-client`, `wayland-protocols`, `wayland-protocols-wlr` | falar direto com o labwc (janelas abertas, áreas de trabalho, aviso de volume) |
| `png` | captura de tela de teste |

Com as dependências delas, o `Cargo.lock` tem **~540 pacotes**. Quase todos vêm
do Slint. Antes de acrescentar uma crate nova, veja se dá para fazer com o que já
existe: cada crate é código de terceiros que roda no computador de quem usa. O
histórico mostra que o projeto fez MD5/SHA-256, Base64 e JSON próprios
(`src/tools/codec.rs`, testados com os vetores oficiais) justamente para não
trazer crates novas.

## Slint em 5 minutos

Um componente:

```slint
import { Theme } from "../theme.slint";

export component Toggle inherits Rectangle {
    in-out property <bool> checked;     // dado que entra e sai
    callback toggled(bool);             // evento que o Rust (ou outra tela) ouve

    width: 44px; height: 24px;
    border-radius: 12px;
    background: root.checked ? Theme.accent : Theme.tile;   // reage sozinho
    animate background { duration: Theme.anim; }

    TouchArea {
        clicked => {
            root.checked = !root.checked;
            root.toggled(root.checked);
        }
    }
}
```

Ideias principais:

- **Propriedades reativas.** `background: root.checked ? … : …` é recalculado
  sozinho quando `checked` muda. Não há "atualizar a tela" manual.
- **`in`, `out`, `in-out`**: quem pode ler e escrever a propriedade.
- **`global`**: dados compartilhados por todas as telas e acessíveis do Rust. Os
  principais são `Theme` (`ui/theme.slint`, cores e opções), `Shell` e `Apps`
  (`ui/globals.slint`), e `Device` (`ui/device.slint`, estado real do computador).
- **`callback`**: o Slint chama, o Rust responde.

### Como o Rust conversa com a tela

```rust
// Ler e escrever propriedades de um global:
let d = ui.global::<Device>();
d.set_volume(0.4);

// Responder a um callback:
d.on_set_volume(move |v| { /* roda wpctl */ });

// Trabalho demorado: nunca na linha da interface.
std::thread::spawn(move || {
    let resultado = rodar_nmcli();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.global::<Device>().set_networks(resultado);
        }
    });
});
```

Regra importante: **comandos lentos (`nmcli`, `lpstat`, `apt`) rodam numa
thread**, e o resultado volta para a tela com `invoke_from_event_loop`. Se
rodarem na linha da interface, a janela congela. O `src/settings/device.rs` é o
exemplo mais completo desse padrão.

### Armadilhas conhecidas do Slint e do winit neste projeto

- Campo que precisa receber foco (`focus()`) não pode estar sob `visible: false`
  nem num `if` recém-criado: o foco se perde no Wayland.
- No Wayland, uma janela escondida (`hide()`) não volta com `show()`. O Alt+Tab
  cria uma janela nova a cada uso por isso.
- `ui.run()` encerra o programa quando a última janela some. A captura de tela
  esconde a janela antes da foto, então usa `run_event_loop_until_quit()`.
- No **renderizador de software**, `drop-shadow` e gradiente ignoram o
  arredondamento (saem quadrados). O shell usa software em máquinas sem placa de
  vídeo e OpenGL nas outras (`lib.rs::renderer_for`).
- Imagens grandes reduzidas pelo OpenGL saem serrilhadas (sem *mipmaps*): ícones
  PNG grandes são reduzidos antes, com filtro Lanczos (`apps::load_icon`).

## Idioma da interface

- Texto na tela é escrito em **inglês** com `@tr("...")` nos `.slint`.
- A tradução fica em `lang/pt_BR/LC_MESSAGES/kalango_os.po` (par
  `msgid`/`msgstr`) e vai embutida no binário.
- Texto montado no Rust usa `i18n::tr("English", "Português")`.
- O idioma vem de `LC_ALL`/`LC_MESSAGES`/`LANG` (`src/common/i18n.rs`).
- Comentários e documentação do código: **português**.

Texto novo na tela = `@tr("Inglês")` + o par no `.po`. Sem o par, aparece em
inglês para todos.

## Convenções do código

- Documentação em todo item público (`///` e `//!`), em português, explicando o
  **porquê**, não só o quê.
- Funções que leem texto de comandos (`parse_*`) são puras e testadas com saídas
  reais coladas no teste. É o que garante que uma mudança de formato num
  programa do Debian seja pega no `cargo test`.
- Caminhos completos nos `use` (`crate::shell::session::…`).
- Commits em português no padrão *Conventional Commits* (`feat(dock): …`,
  `fix(seguranca): …`) e assinados com `git commit -s` (DCO).

## Como acrescentar uma página nas Configurações

1. Estado e callbacks num global (`ui/device.slint` ou um global novo).
2. A página em `ui/screens/settings_pages.slint` e a entrada na barra lateral
   em `ui/screens/settings.slint` (a ordem segue `order`; não renumere páginas
   existentes, porque o menu da área de trabalho abre páginas pelo número).
3. Um módulo `src/settings/<assunto>.rs` com `install(ui, mode)`, chamado em
   `lib.rs::build_mode`.
4. Se precisar de root: um script em `distro/config/includes.chroot/usr/local/sbin/`,
   uma política em `usr/share/polkit-1/actions/` e as regras de
   [Segurança](seguranca.md#scripts-que-rodam-como-root).
5. Textos no `.po` e testes para o que lê saída de comando.
