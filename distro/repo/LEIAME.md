# Repositório apt do Kalango OS (atualizações OTA)

O sistema instalado procura pacotes do projeto em
`https://kalangos.easysmartcontroll.com/apt` (`/etc/apt/sources.list.d/kalango.sources`,
chave em `/usr/share/keyrings/kalango-archive-keyring.gpg`). Uma versão nova
publicada lá aparece em **Configurações → Atualizações** como as do Debian.

## Uma vez

1. `distro/repo/criar-chave.sh`: cria a chave de assinatura em
   `~/.local/share/kalango-repo/gnupg` e grava a pública na ISO.
   **Guarde uma cópia dessa pasta** (sem ela, os sistemas instalados não aceitam
   pacotes assinados por outra chave).
2. DNS: registro `A` de `kalangos.easysmartcontroll.com` para o VPS (feito).
3. No VPS (aaPanel, SSH `root@10.8.0.1` pela VPN): nginx com `distro/repo/nginx-kalangos.conf`
   + certificado do certbot por webroot (feito em 2026-11-15).

## A cada versão

```sh
# 1. suba a versão no Cargo.toml (ex.: 0.1.1) ou use KALANGO_REVISAO=2 para
#    republicar a mesma versão com correções
distro/build.sh --so-shell              # compila e gera distro/out/kalango-shell_<versão>_amd64.deb
# 2. assina e envia
RSYNC_RSH="ssh -i ~/.ssh/id_tiago_desktop" \
KALANGO_REPO_DESTINO=root@10.8.0.1:/www/wwwroot/kalangos.easysmartcontroll.com/apt \
    distro/repo/publicar.sh distro/out/kalango-shell_*.deb
```

A ISO (`distro/build.sh`) instala o mesmo pacote e deixa uma cópia do `.deb` em
`distro/out/`: publique-a junto com cada ISO, para que o repositório nunca tenha
versão mais velha que a da ISO.

O pacote leva o binário e os scripts de `/usr/local/{bin,sbin,lib/kalango}` e o
hook `kalango-radios`; os arquivos de `/etc` continuam só na ISO.
