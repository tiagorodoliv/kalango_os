# Manutenção

Rotina para o projeto continuar funcionando, atualizado e seguro.

## Toda mudança

- [ ] `cargo test` e `cargo clippy --all-targets -- -D warnings` passam.
- [ ] Texto novo na tela tem tradução no `.po`.
- [ ] Mudou script de root? Seguiu as [regras](seguranca.md#scripts-que-rodam-como-root)
      e testou o caso malicioso.
- [ ] Mudou algo em `/etc` ou na lista de pacotes? Pensou em quem já tem o sistema
      instalado (só a ISO leva isso; veja [Sistema](sistema-e-hardware.md#onde-ficam-as-mudanças-no-sistema)).
- [ ] Opção nova de aparência? Valor antigo em `appearance::LEGACY`.
- [ ] Testou na VM (`distro/testar-vm.sh`) o que mexe em login, bloqueio ou boot.
- [ ] Commit em português, *Conventional Commits*, assinado (`git commit -s`).
- [ ] Decisão nova ou armadilha descoberta: uma linha no `CLAUDE.md`.

## Cada versão publicada no apt

1. Atualize `distro/pacotes/novidades` (português e inglês).
2. `cargo audit` sem vulnerabilidades.
3. Build de release numa **cópia limpa** do commit:
   ```sh
   git worktree add /tmp/kalango-limpo HEAD
   cd /tmp/kalango-limpo && cargo build --release --locked
   ```
4. `KALANGO_REVISAO=<próxima> distro/pacotes/gerar-deb.sh <binário> <pasta>`.
   Mudou foto? Suba a versão em `gerar-deb-papeis.sh` e gere também esse pacote.
5. `distro/repo/publicar.sh <pacotes>` e confira que o servidor mostra a versão nova.
6. Atualize uma máquina pela página Atualizações e veja as novidades e o
   "Recarregar agora".
7. `git worktree remove` da cópia limpa.

## Cada ISO

1. `distro/gerar-isos.sh todas --enviar` (usa o pacote publicado).
2. Teste as duas no QEMU (BIOS, UEFI, Secure Boot) e instale numa VM com disco.
3. Ponha links, tamanhos e SHA-256 no site e publique (`kalangos-site/publicar.sh`).
4. Codinome novo a cada série (`distro/codinomes`, plantas do sertão).

## Todo mês

- [ ] `cargo update`, testes e um teste visual na VM. Leia o changelog do Slint.
- [ ] `cargo audit`.
- [ ] Republicar o apt (quando houver `Valid-Until`).
- [ ] Atualizações do servidor (`apt upgrade` no VPS) e conferir `sshd -T`.
- [ ] Olhar se há kernel/Mesa novos relevantes nos backports (edição HWE).

## A cada lançamento do Debian

- **Ponto (13.x):** nada a fazer, chega pelo apt. Gere uma ISO nova de vez em
  quando para quem instala não baixar tudo de novo.
- **Versão nova (Debian 14 "forky", ~2027):** é uma migração grande. Labwc, Slint,
  GTK e o Calamares mudam de versão; vários comportamentos documentados aqui
  (limitações do labwc 0.8, Calamares 3.3) precisam ser revistos. Planeje uma
  série nova (0.x com codinome novo) e uma rota de atualização testada em VM.

## Quando algo quebra numa máquina

| Onde olhar | Comando |
|---|---|
| Interface | `journalctl -t kalango-shell` |
| Compositor | `journalctl -t labwc` |
| Login | `journalctl -u greetd` |
| Rádios ao voltar da suspensão | `journalctl -t kalango-radios` |
| Atualização | `/var/log/apt/term.log` |
| Versão anterior em uso? | aviso na barra (`KALANGO_SHELL_ANTERIOR=1`) |

## Onde está o conhecimento

- O **porquê** de cada detalhe estranho está no comentário `//!` ou `///` do
  próprio código e no `CLAUDE.md`. Antes de "simplificar" algo que parece
  desnecessário, leia o comentário: quase sempre é uma limitação do labwc, do
  Wayland ou de um programa do Debian que já causou um problema.
- Esta pasta `docs/` é o mapa; mantenha-a atualizada quando a estrutura mudar
  (pasta nova, pacote novo, script de root novo).
