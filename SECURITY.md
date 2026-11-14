# Segurança

## Como relatar uma falha

**Não abra uma issue pública** para falhas de segurança. Use o relato privado do GitHub:
aba **Security** do repositório → **Report a vulnerability**. Só o mantenedor vê o relato.

Conte o que encontrou, como reproduzir, a versão da ISO e o impacto (ex.: entrar sem senha, rodar comando
como root, ler arquivo de outro usuário). A resposta inicial vem em até 7 dias.

## O que é mais sensível

- **Login e bloqueio:** o greeter (`src/shell/greeter.rs`, IPC do greetd/PAM) e o `swaylock`.
- **Instalador:** o Calamares e os comandos em `distro/config/includes.chroot/etc/calamares/`.
- **Configurações:** comandos de sistema rodados pelo shell (`src/settings/device.rs`, `src/shell/session.rs`).
- **Configuração da ISO live:** usuário `user` com senha `live` e `sudo` sem senha. Isso é proposital só na
  mídia live (como em outras distros) e não pode sobrar no sistema instalado.

## Versões com suporte

Só a versão mais recente recebe correções de segurança. Os pacotes do Debian recebem atualizações pelo
repositório de segurança do Debian (`apt full-upgrade` ou a tela **Atualizações**).
