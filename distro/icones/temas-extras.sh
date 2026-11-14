#!/bin/sh
# Instala os temas de ícones extras do Kalango OS (escolhidos em
# Configurações → Aparência): Colloid e Fluent, ambos GPL-3.0, de
# https://github.com/vinceliuice.
#
# Para cada tema:
#   - <Base>: a variante escura na cor padrão (azul), inteira;
#   - Kalango-<Tema>-<Cor>: no Colloid, só os arquivos que mudam na cor de
#     destaque (pastas e alguns apps), herdando o resto da base; no Fluent,
#     um nome que aponta para a variante daquela cor. O shell troca entre
#     eles junto com a cor de destaque (src/settings/appearance.rs, icon_theme_name).
#
# Uso: distro/icones/temas-extras.sh <pasta_icons_de_destino> <pasta_de_cache>
# Chamado pelo distro/build.sh. Versões fixas (commit + SHA-256).
set -eu

DESTINO=$1
CACHE=$2
mkdir -p "$CACHE" "$DESTINO"

# Cor de destaque do Kalango -> nome da cor no tema (ordem de Theme.accents).
CORES="Ciano Roxo Azul Verde Ambar Rosa"

# baixar <nome> <url> <sha256>: baixa para o cache (uma vez) e confere.
baixar() {
	arq="$CACHE/$1.tar.gz"
	# Só o caminho vai para a saída (quem chama usa $(baixar ...)); o resto
	# vai para o stderr.
	if ! echo "$3  $arq" | sha256sum -c --status >/dev/null 2>&1; then
		echo ">> Baixando $1" >&2
		curl -fsSL -o "$arq.part" "$2"
		echo "$3  $arq.part" | sha256sum -c --status >/dev/null 2>&1 || {
			echo "$1: SHA-256 não confere" >&2
			rm -f "$arq.part"
			exit 1
		}
		mv "$arq.part" "$arq"
	fi
	echo "$arq"
}

# sobrepor <base> <variante> <tema_kalango> <nome_exibido>: copia para
# <tema_kalango> só os arquivos da variante que diferem da base (e os atalhos
# que apontam para eles) e escreve o index.theme herdando da base.
sobrepor() {
	base="$DESTINO/$1"
	var=$2
	tema="$DESTINO/$3"
	rm -rf "$tema"
	mkdir -p "$tema"
	(cd "$var" && find . -type f -name '*.svg') | while read -r f; do
		if ! cmp -s "$var/$f" "$base/$f"; then
			mkdir -p "$tema/$(dirname "$f")"
			cp "$var/$f" "$tema/$f"
		fi
	done
	# Atalhos (a -> b): vale o que aponta para um arquivo copiado.
	(cd "$var" && find . -type l -name '*.svg' ! -xtype d) | while read -r f; do
		alvo=$(readlink "$var/$f")
		if [ ! -e "$tema/$f" ] && [ -e "$tema/$(dirname "$f")/$alvo" ]; then
			ln -s "$alvo" "$tema/$f"
		fi
	done
	sed -e "s/^Name=.*/Name=$4/" -e "s/^Inherits=.*/Inherits=$1,Adwaita,hicolor/" \
		"$base/index.theme" > "$tema/index.theme"
	grep -q '^Inherits=' "$tema/index.theme" ||
		sed -i "s/^\[Icon Theme\]/[Icon Theme]\nInherits=$1,Adwaita,hicolor/" "$tema/index.theme"
	cp "$base/COPYING" "$tema/" 2>/dev/null || true
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# --- Colloid (estilo macOS com desenho próprio; -a = Loja e Arquivos sem
# imitar a App Store e o Finder) ---------------------------------------------
COLLOID=ceac6608ecd0e40025cbc2ebbd32bf0e0f4ebc6a
pacote=$(baixar "colloid-$COLLOID" \
	"https://github.com/vinceliuice/Colloid-icon-theme/archive/$COLLOID.tar.gz" \
	3f731f9f13b1c1403893561ff92adec7b3ab07c5495f55c78c111b07a10a296e)
mkdir -p "$TMP/colloid" "$TMP/colloid-out"
tar xzf "$pacote" -C "$TMP/colloid" --strip-components=1
for par in Ciano:teal Roxo:purple Azul:default Verde:green Ambar:yellow Rosa:pink; do
	(cd "$TMP/colloid" && bash ./install.sh -d "$TMP/colloid-out" -t "${par#*:}" -a >/dev/null)
done
# A variante escura reaproveita pastas da clara por atalhos
# (Colloid-Dark/apps/scalable -> ../../Colloid-Light/...): vão as duas.
for base in Colloid-Dark Colloid-Light; do
	rm -rf "${DESTINO:?}/$base"
	cp -a "$TMP/colloid-out/$base" "$DESTINO/$base"
done
cp "$TMP/colloid/COPYING" "$DESTINO/Colloid-Dark/" 2>/dev/null || cp "$TMP/colloid/LICENSE" "$DESTINO/Colloid-Dark/COPYING"
for par in Ciano:Teal Roxo:Purple Azul: Verde:Green Ambar:Yellow Rosa:Pink; do
	cor=${par%%:*}
	sufixo=${par#*:}
	nome="Colloid${sufixo:+-$sufixo}-Dark"
	sobrepor Colloid-Dark "$TMP/colloid-out/$nome" "Kalango-Colloid-$cor" "Kalango Colloid $cor"
done

# --- Fluent (estilo Windows 11, colorido) -----------------------------------
FLUENT=c8a244f571ef8fb47812bdc9dbce76546cedce69
pacote=$(baixar "fluent-$FLUENT" \
	"https://github.com/vinceliuice/Fluent-icon-theme/archive/$FLUENT.tar.gz" \
	07b6b593ab557e07f0a7cdb232cda99214af13fb74ad6ce1e77d666057647767)
mkdir -p "$TMP/fluent" "$TMP/fluent-out"
tar xzf "$pacote" -C "$TMP/fluent" --strip-components=1
(cd "$TMP/fluent" && bash ./install.sh -d "$TMP/fluent-out" teal purple standard green yellow pink >/dev/null)
# O Fluent guarda os ícones comuns em ".Fluent-base" e ".Fluent-dark-base"
# (ocultas) e cada cor é
# uma pasta com atalhos para ela; copiar com os atalhos (-a) mantém tudo
# pequeno. Kalango-Fluent-<Cor> só aponta (Inherits) para a cor certa.
for base in .Fluent-base .Fluent-dark-base; do
	rm -rf "${DESTINO:?}/$base"
	cp -a "$TMP/fluent-out/$base" "$DESTINO/$base"
done
for par in Ciano:teal Roxo:purple Azul: Verde:green Ambar:yellow Rosa:pink; do
	cor=${par%%:*}
	sufixo=${par#*:}
	nome="Fluent${sufixo:+-$sufixo}"
	rm -rf "$DESTINO/$nome" "$DESTINO/$nome-dark"
	cp -a "$TMP/fluent-out/$nome" "$TMP/fluent-out/$nome-dark" "$DESTINO/"
	tema="$DESTINO/Kalango-Fluent-$cor"
	rm -rf "$tema"
	mkdir -p "$tema/places/scalable"
	cp "$TMP/fluent/COPYING" "$tema/" 2>/dev/null || true
	cat > "$tema/index.theme" <<-EOT
	[Icon Theme]
	Name=Kalango Fluent $cor
	Comment=Ícones Fluent com as pastas na cor de destaque $cor
	Inherits=$nome-dark,Adwaita,hicolor
	Example=folder
	Directories=places/scalable

	[places/scalable]
	Context=Places
	Size=16
	MinSize=8
	MaxSize=512
	Type=Scalable
	EOT
done

echo ">> Ícones extras instalados em $DESTINO (Colloid, Fluent e Kalango-<Tema>-<Cor>)"
