# Repositório apt do KalangOS (atualizações OTA)

O sistema instalado procura pacotes do projeto em
`https://kalangos.org/apt` (`/etc/apt/sources.list.d/kalango.sources`,
chave em `/usr/share/keyrings/kalango-archive-keyring.gpg`). Uma versão nova
publicada lá aparece em **Configurações → Atualizações** como as do Debian.

## Uma vez

1. `distro/repo/criar-chave.sh`: cria a chave de assinatura em
   `~/.local/share/kalango-repo/gnupg` e grava a pública na ISO.
   **Guarde uma cópia dessa pasta** (sem ela, os sistemas instalados não aceitam
   pacotes assinados por outra chave).
2. DNS: registro `A` de `kalangos.org` para o VPS (feito). Até 2026-11-17 o
   endereço era `kalangos.easysmartcontroll.com/apt`; ele continua servindo os
   mesmos arquivos (no VPS, `/www/wwwroot/kalangos.org/apt` é um link para
   `/www/wwwroot/kalangos.easysmartcontroll.com/apt`) e o postinst do
   `kalango-shell` passa os sistemas antigos para o endereço novo.
3. No VPS (aaPanel, acesso por SSH): nginx com `distro/repo/nginx-kalangos.conf`
   + certificado do certbot por webroot (feito em 2026-11-15).

## A cada versão

```sh
# 1. suba a versão no Cargo.toml (ex.: 0.1.1) ou use KALANGO_REVISAO=2 para
#    republicar a mesma versão com correções
distro/build.sh --so-shell              # compila e gera distro/out/kalango-shell_<versão>_amd64.deb
# 2. assina e envia (traz antes do servidor os pacotes publicados de outra
#    máquina: o envio usa --delete). KALANGO_REPO = pasta da chave, se não for
#    a padrão (no desktop do Tiago: ~/Documents/.local/share/kalango-repo)
RSYNC_RSH="ssh -i ~/.ssh/SUA_CHAVE" \
KALANGO_REPO_DESTINO=root@SEU_VPS:/www/wwwroot/kalangos.easysmartcontroll.com/apt \
    distro/repo/publicar.sh distro/out/kalango-shell_*.deb
```

A ISO (`distro/build.sh`) instala o mesmo pacote e deixa uma cópia do `.deb` em
`distro/out/`: publique-a junto com cada ISO, para que o repositório nunca tenha
versão mais velha que a da ISO.

O pacote leva o binário e os scripts de `/usr/local/{bin,sbin,lib/kalango}`, o
hook `kalango-radios` e o terminal (temas do foot em `/usr/share/kalango/terminal`
e o `.zshrc` no estilo do Kali; o postinst passa as contas do bash para o zsh uma
única vez por máquina). Os demais arquivos de `/etc` continuam só na ISO.

## Sem o desktop Ubuntu (compilação cruzada)

Qualquer Debian 13 serve para compilar o pacote, até um arm64 (VM no Mac): com
`dpkg --add-architecture amd64`, `crossbuild-essential-amd64` e as bibliotecas
`libfontconfig-dev:amd64 libfreetype-dev:amd64 libxkbcommon-dev:amd64
libwayland-dev:amd64`, rode `cargo build --release --locked --target
x86_64-unknown-linux-gnu` com `CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc`,
`PKG_CONFIG_ALLOW_CROSS=1` e `PKG_CONFIG_LIBDIR=/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/share/pkgconfig`.
O binário sai contra a glibc do trixie, como o do `build.sh`.
