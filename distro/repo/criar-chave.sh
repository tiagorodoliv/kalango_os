#!/bin/sh
# Cria a chave GPG que assina o repositório apt do KalangOS (uma vez só).
#
# A chave privada fica NESTA máquina, em ~/.local/share/kalango-repo/gnupg
# (o VPS só serve arquivos já assinados). Faça cópia de segurança dessa pasta:
# sem ela, quem já instalou o sistema não aceita pacotes assinados por outra.
# A pública vai para a ISO em
# distro/config/includes.chroot/usr/share/keyrings/kalango-archive-keyring.gpg.
set -eu

REPO=${KALANGO_REPO:-$HOME/.local/share/kalango-repo}
export GNUPGHOME="$REPO/gnupg"
NOME="KalangOS (repositório apt) <repo@kalangos.easysmartcontroll.com>"
DISTRO=$(cd "$(dirname "$0")/.." && pwd)
CHAVEIRO="$DISTRO/config/includes.chroot/usr/share/keyrings/kalango-archive-keyring.gpg"

mkdir -p "$GNUPGHOME"
chmod 700 "$GNUPGHOME"
if ! gpg --batch --list-secret-keys "$NOME" >/dev/null 2>&1; then
	gpg --batch --passphrase '' --quick-gen-key "$NOME" ed25519 sign 10y
fi
mkdir -p "$(dirname "$CHAVEIRO")"
gpg --batch --export "$NOME" >"$CHAVEIRO"
gpg --batch --list-keys --keyid-format long "$NOME"
echo "chave pública: $CHAVEIRO"
