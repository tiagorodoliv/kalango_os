#!/bin/sh
# Monta o pacote kalango-papeis-de-parede (.deb, arquitetura all): as fotos em
# JPEG que viram modelos de papel de parede (assets/wallpapers/16x9/Kalango_*.jpg,
# feitas por assets/wallpapers/fotos.py) em /usr/share/backgrounds/kalango.
#
# Fica separado do kalango-shell porque é grande (~30 MB) e muda pouco: uma
# correção na interface não faz ninguém baixar as fotos de novo. O kalango-shell
# depende dele. Os modelos desenhados (PNG) continuam só na ISO.
#
# O pacote é reprodutível: a mesma VERSAO gera o mesmo arquivo, byte a byte
# (datas fixas, dono root). Assim o build.sh pode gerá-lo de novo para a ISO sem
# conflitar com o publicado no apt. Mudou alguma foto → suba a VERSAO.
#
# Uso: distro/pacotes/gerar-deb-papeis.sh PASTA_DE_SAIDA
# Saída: PASTA_DE_SAIDA/kalango-papeis-de-parede_<versão>_all.deb
set -eu

# Suba a cada mudança nas fotos (formato data.revisão).
VERSAO=2026.11.20-1
# Data fixa dos arquivos e do pacote (reprodutível): meia-noite do dia da versão.
EPOCH=1795143600

SAIDA=${1:?pasta de saída}
DISTRO=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(dirname "$DISTRO")

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
P="$TMP/pkg"
mkdir -p "$P/DEBIAN" "$P/usr/share/backgrounds/kalango"
cp "$ROOT"/assets/wallpapers/16x9/Kalango_*.jpg "$P/usr/share/backgrounds/kalango/"
chmod 0644 "$P"/usr/share/backgrounds/kalango/*
# Permissões fixas (não dependem do umask de quem gera: root no build.sh).
find "$P" -type d -exec chmod 0755 {} +

cat >"$P/DEBIAN/control" <<EOF
Package: kalango-papeis-de-parede
Version: $VERSAO
Architecture: all
Maintainer: KalangOS <repo@kalangos.org>
Installed-Size: $(du -sk --apparent-size "$P/usr" | cut -f1)
Replaces: kalango-shell (<< 0.1.1-17)
Breaks: kalango-shell (<< 0.1.1-17)
Section: graphics
Priority: optional
Homepage: https://kalangos.org/
Description: Papéis de parede do KalangOS (fotos do Sertão e da Chapada)
 Modelos de papel de parede em foto, em 1920x1080, 2560x1440 e 3840x2160,
 escolhidos em Configurações > Aparência.
EOF

# Datas e ordem fixas para o arquivo sair igual a cada geração.
find "$P" -exec touch -h -d "@$EPOCH" {} +
mkdir -p "$SAIDA"
DEB="$SAIDA/kalango-papeis-de-parede_${VERSAO}_all.deb"
SOURCE_DATE_EPOCH=$EPOCH dpkg-deb --root-owner-group -Zxz --build "$P" "$DEB" >/dev/null
echo "$DEB"
