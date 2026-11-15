# Como contribuir com o KalangOS

Obrigado por querer ajudar! Toda contribuição passa por revisão do mantenedor antes de entrar no `main`,
e as versões (ISOs) são publicadas por ele.

## Jeitos de ajudar

- **Relatar um problema:** abra uma *issue* com o modelo "Relatar um problema". Diga o que fez, o que esperava
  e o que aconteceu, a versão da ISO e se é em VM ou máquina real (marca/modelo).
  Logs ajudam muito: `journalctl -b -t kalango-shell` (shell) e `journalctl -b -u greetd` (login).
- **Sugerir um recurso:** abra uma *issue* com o modelo "Sugerir um recurso" antes de começar a programar
  algo grande, para combinar a ideia.
- **Mandar código, tradução, ícone ou documentação:** pelo fluxo de Pull Request abaixo.
- **Testar** as ISOs em máquinas reais e contar o que funcionou (Wi-Fi, som, vídeo, suspender).

Falhas de segurança **não** vão em issue pública: veja [SECURITY.md](SECURITY.md).

## Fluxo de Pull Request

1. Faça um *fork* do repositório e crie um *branch* a partir do `main` (`git switch -c fix/wifi-senha`).
2. Faça a mudança, com testes quando der.
3. Confira localmente:
   ```sh
   cargo test
   cargo clippy --all-targets -- -D warnings
   cargo run --example snapshot   # se mexeu na interface: confira as capturas em target/snapshots/
   ```
4. Abra o Pull Request para o `main` preenchendo o modelo. O CI roda os testes sozinho.
5. O mantenedor revisa, pode pedir ajustes e faz o *merge* quando estiver pronto.

Prefira PRs pequenos e com um assunto só: são revisados mais rápido.

## Padrões do projeto

- **Idioma:** código, comentários, documentação, mensagens de commit e textos da interface em **português
  do Brasil**. Nomes de variáveis e funções podem ser em inglês (é o que o código já usa).
- **Commits:** [Conventional Commits](https://www.conventionalcommits.org/pt-br/), em pt-BR, no imperativo:
  `feat(shell): ...`, `fix(distro): ...`, `docs(projeto): ...`, `refactor(...)`, `test(...)`, `chore(...)`.
  Escopos usados: `shell` (src/ e ui/), `distro` (distro/), `projeto` (docs gerais), `snapshot`.
- **Documentação:** tudo que é público no Rust leva comentário `///` (rustdoc); componentes e globais do
  Slint também. Comente o *porquê* das decisões não óbvias (ex.: contornos de bugs do winit ou do nmcli).
- **Interface:** siga o tema em `ui/theme.slint` (cores e fontes) e os layouts de referência em `assets/layout/`.
  Use os arquivos reais da marca (`ui/brand/`), nunca um desenho próprio do logo.
- **Sistema:** a distro é Debian 13 (trixie) gerada com `live-build`; pacotes em
  `distro/config/package-lists/`, configurações em `distro/config/includes.chroot/`.

## Ambiente de desenvolvimento

Veja o [README](README.md): Rust via `rustup`, `cargo run` abre o protótipo numa janela, e `distro/build.sh`
gera a ISO (precisa de Linux, `sudo`, `debootstrap` e `debian-archive-keyring`). Para testar a ISO,
`distro/run-qemu.sh` (QEMU/KVM).

## Licença das contribuições

O KalangOS é licenciado sob a [GPL-3.0](LICENSE). Ao mandar uma contribuição, você concorda que ela seja
distribuída sob essa mesma licença e declara que tem o direito de enviá-la (o código é seu ou compatível
com a GPL-3.0). Para registrar isso, assine os commits com `git commit -s`, que acrescenta a linha
`Signed-off-by` ([Developer Certificate of Origin](https://developercertificate.org/)).

O nome e o logo do KalangOS seguem regras próprias: veja [TRADEMARKS.md](TRADEMARKS.md).

## Convivência

Este projeto segue o [Código de Conduta](CODE_OF_CONDUCT.md). Seja gentil, principalmente com quem está
começando.
