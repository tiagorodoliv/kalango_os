#!/bin/sh
# Botão "Trocar de usuário" da tela de bloqueio: gtklock-powerbar-module
# (GPL-3.0, Jovan Lanik), que não está no Debian 13. Versão fixa, conferida
# pelo SHA-256, compilada contra o gtklock 4.0 do trixie.
#
# Uso:
#   powerbar.sh baixar CACHE
#       Garante CACHE/gtklock-powerbar-module-<versão>.tar.gz (baixa uma vez)
#       e escreve o caminho dele na saída padrão.
#   powerbar.sh compilar TARBALL DESTINO
#       Compila num Debian trixie (instala libgtk-3-dev se faltar) e grava
#       DESTINO/powerbar-module.so. O build.sh roda isso dentro do builder.
#
# O texto do botão ("Switch user" no original) passa a vir da variável
# KALANGO_TROCAR_USUARIO, que o kalango-bloquear põe no idioma da sessão.
set -eu

# Baixa $1 para $2 com o curl ou, sem ele (o KalangOS instalado só tem o
# wget), com o wget.
baixar_url() {
	if command -v curl >/dev/null 2>&1; then
		curl -fsSL -o "$2" "$1"
	else
		wget -q -O "$2" "$1"
	fi
}

VERSAO=4.0.0
SHA256=7fefa48ddb9a60e58b5c72df6da2005e1dd38fdc779c5d535aac80da9b508e9e
URL=https://github.com/jovanlanik/gtklock-powerbar-module/archive/refs/tags/v$VERSAO.tar.gz
NOME=gtklock-powerbar-module-$VERSAO

case "${1:-}" in
baixar)
	CACHE=$2
	ARQ="$CACHE/$NOME.tar.gz"
	mkdir -p "$CACHE"
	if [ ! -f "$ARQ" ] || ! echo "$SHA256  $ARQ" | sha256sum -c --quiet - 2>/dev/null; then
		echo ">> Baixando $NOME" >&2
		baixar_url "$URL" "$ARQ.part"
		echo "$SHA256  $ARQ.part" | sha256sum -c --quiet -
		mv "$ARQ.part" "$ARQ"
	fi
	echo "$ARQ"
	;;
compilar)
	TARBALL=$2
	DESTINO=$3
	echo "$SHA256  $TARBALL" | sha256sum -c --quiet -
	if ! pkg-config --exists gtk+-3.0 gmodule-export-2.0 2>/dev/null; then
		DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
			libgtk-3-dev gcc pkg-config >/dev/null
	fi
	TMP=$(mktemp -d)
	trap 'rm -rf "$TMP"' EXIT
	tar -xzf "$TARBALL" -C "$TMP"
	cd "$TMP/$NOME"
	sed -i 's/"Switch user"/(g_getenv("KALANGO_TROCAR_USUARIO") ? g_getenv("KALANGO_TROCAR_USUARIO") : "Switch user")/' source.c
	grep -q KALANGO_TROCAR_USUARIO source.c
	# shellcheck disable=SC2046
	gcc -std=c11 -O2 -shared -fPIC -o powerbar-module.so source.c \
		$(pkg-config --cflags --libs gtk+-3.0 gmodule-export-2.0)
	install -D -m 0644 powerbar-module.so "$DESTINO/powerbar-module.so"
	echo ">> $DESTINO/powerbar-module.so"
	;;
*)
	echo "uso: powerbar.sh baixar CACHE | compilar TARBALL DESTINO" >&2
	exit 1
	;;
esac
