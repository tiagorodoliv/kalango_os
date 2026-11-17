#!/bin/sh
# Instala os ícones do KalangOS: o tema Flat Remix (GPL-3.0,
# https://github.com/daniruiz/flat-remix) e um tema pequeno por cor de
# destaque, Kalango-<Cor>, só com as pastas na cor certa.
#
# - Flat-Remix-Blue-Dark: o tema inteiro (apps, arquivos, dispositivos...);
# - Kalango-Ciano, -Roxo, -Azul, -Verde, -Ambar, -Rosa: a pasta `places` da
#   variante do Flat Remix na cor do destaque, herdando o resto do
#   Flat-Remix-Blue-Dark. O shell troca entre eles junto com a cor de
#   destaque (src/settings/appearance.rs, ICON_THEMES).
#
# Uso: distro/icones/flat-remix.sh <pasta_icons_de_destino> <pasta_de_cache>
# Chamado pelo distro/build.sh. O pacote baixado fica no cache e é conferido
# pelo SHA-256 (versão fixa: atualizar VERSAO e SHA256 juntos).
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

VERSAO=20251119
SHA256=eba0547045a1714b47f1951d342624b70da150bdcc69d480883ab4667fe3e09b
URL="https://github.com/daniruiz/flat-remix/archive/refs/tags/$VERSAO.tar.gz"
BASE=Flat-Remix-Blue-Dark

DESTINO=$1
CACHE=$2
PACOTE="$CACHE/flat-remix-$VERSAO.tar.gz"

mkdir -p "$CACHE" "$DESTINO"
if ! echo "$SHA256  $PACOTE" | sha256sum -c --status 2>/dev/null; then
	echo ">> Baixando o Flat Remix $VERSAO"
	baixar_url "$URL" "$PACOTE.part"
	echo "$SHA256  $PACOTE.part" | sha256sum -c --status || {
		echo "Flat Remix: SHA-256 não confere" >&2
		rm -f "$PACOTE.part"
		exit 1
	}
	mv "$PACOTE.part" "$PACOTE"
fi

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
# Cor de destaque do Kalango -> variante do Flat Remix (mesma ordem de
# Theme.accents em ui/theme.slint).
CORES="Ciano:Cyan Roxo:Violet Azul:Blue Verde:Green Ambar:Yellow Rosa:Magenta"
PADROES="flat-remix-$VERSAO/LICENSE flat-remix-$VERSAO/$BASE"
for par in $CORES; do
	# A variante azul já vem inteira como base.
	[ "${par#*:}" = Blue ] && continue
	PADROES="$PADROES flat-remix-$VERSAO/Flat-Remix-${par#*:}-Dark/places"
done
# shellcheck disable=SC2086
tar xzf "$PACOTE" -C "$TMP" $PADROES
ORIGEM="$TMP/flat-remix-$VERSAO"

rm -rf "${DESTINO:?}/$BASE"
mv "$ORIGEM/$BASE" "$DESTINO/$BASE"
cp "$ORIGEM/LICENSE" "$DESTINO/$BASE/LICENSE"

for par in $CORES; do
	cor=${par%%:*}
	variante=${par#*:}
	tema="$DESTINO/Kalango-$cor"
	rm -rf "$tema"
	mkdir -p "$tema"
	mkdir -p "$tema/places/scalable"
	if [ "$variante" != Blue ]; then
		# Só o que muda em relação à base azul (as pastas), mais os atalhos
		# (symlinks) que apontam para esses arquivos.
		de="$ORIGEM/Flat-Remix-$variante-Dark/places/scalable"
		para="$tema/places/scalable"
		for f in "$de"/*; do
			nome=$(basename "$f")
			if [ ! -L "$f" ] && ! cmp -s "$f" "$DESTINO/$BASE/places/scalable/$nome"; then
				cp "$f" "$para/$nome"
			fi
		done
		# Atalhos em cadeia (a -> b -> c): repete até não sobrar nenhum novo.
		novos=1
		while [ "$novos" -gt 0 ]; do
			novos=0
			for f in "$de"/*; do
				nome=$(basename "$f")
				[ -L "$f" ] && [ ! -e "$para/$nome" ] || continue
				alvo=$(readlink "$f")
				if [ -e "$para/$alvo" ]; then
					ln -s "$alvo" "$para/$nome"
					novos=$((novos + 1))
				fi
			done
		done
	fi
	cp "$ORIGEM/LICENSE" "$tema/LICENSE"
	cat > "$tema/index.theme" <<-EOF
	[Icon Theme]
	Name=Kalango $cor
	Comment=Ícones do KalangOS (Flat Remix) com as pastas na cor de destaque $cor
	Inherits=$BASE,Adwaita,hicolor
	Example=folder
	Directories=places/scalable

	[places/scalable]
	Context=Places
	Size=512
	MinSize=16
	MaxSize=512
	Type=Scalable
	EOF
done
echo ">> Ícones instalados em $DESTINO ($BASE e Kalango-<Cor>)"
