#!/bin/sh
# Publica pacotes no repositório apt do KalangOS (atualizações OTA).
#
# Monta o repositório em ~/.local/share/kalango-repo/www/apt (pool/ + dists/),
# assina com a chave de criar-chave.sh (InRelease e Release.gpg) e, com
# KALANGO_REPO_DESTINO, envia por rsync para o VPS, que só serve os arquivos.
#
# Uso:
#   distro/repo/publicar.sh kalango-shell_0.1.1-1_amd64.deb [...]
#   KALANGO_REPO_DESTINO=usuario@vps:/var/www/kalangos/apt distro/repo/publicar.sh ...
# Sem .deb: só reassina e envia o que já está lá.
# Servidor novo, ainda vazio: KALANGO_REPO_NOVO=1 na primeira publicação.
#
# No sistema: /etc/apt/sources.list.d/kalango.sources aponta para
# https://kalangos.org/apt (suite trixie, componente main; até 2026-11-17 era
# kalangos.easysmartcontroll.com/apt, que continua servindo os mesmos arquivos).
#
# Antes de montar, com KALANGO_REPO_DESTINO, traz do servidor os pacotes que já
# estão lá: o envio usa --delete, e uma cópia local atrasada (publicada de outra
# máquina) apagaria do servidor as versões que ela não tem. Só entram os que o
# índice assinado do servidor lista com o mesmo SHA-256; qualquer outro arquivo
# em pool/ interrompe tudo antes de assinar (ver abaixo).
set -eu

REPO=${KALANGO_REPO:-$HOME/.local/share/kalango-repo}
export GNUPGHOME="$REPO/gnupg"
NOME="repo@kalangos.easysmartcontroll.com"
WWW="$REPO/www/apt"
SUITE=trixie
DIST="$WWW/dists/$SUITE"

gpg --batch --list-secret-keys "$NOME" >/dev/null 2>&1 ||
	{ echo "Sem a chave do repositório: rode distro/repo/criar-chave.sh" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

# Pacotes que já estão no servidor (publicados de outra máquina) só entram se
# este mesmo script os assinou antes: o SHA-256 de cada .deb tem de estar no
# Packages do servidor, e o Packages tem de bater com o InRelease assinado pela
# chave do projeto. O servidor só serve arquivos e é o elo fraco (outros sites
# rodam como o mesmo usuário www, há FTP): sem esta conferência, um .deb posto
# lá por um invasor seria assinado aqui na próxima publicação e instalado como
# root em todas as máquinas.
if [ -n "${KALANGO_REPO_DESTINO:-}" ]; then
	gpg --batch --export "$NOME" >"$TMP/chave.gpg"
	mkdir -p "$TMP/servidor" "$TMP/pool" "$WWW/pool"
	# Índice e pacotes do servidor. Não conseguir lê-los é erro de rede ou de
	# acesso: seguir como se o servidor estivesse vazio faria o envio
	# (--delete) apagar os pacotes de lá. Só a primeira publicação num
	# servidor novo pode começar sem eles (KALANGO_REPO_NOVO=1).
	# Os pacotes vão para uma pasta de quarentena; os que já existem aqui
	# iguais são copiados localmente (--copy-dest), sem baixar de novo.
	if ! rsync -q "$KALANGO_REPO_DESTINO/dists/$SUITE/InRelease" \
		"$KALANGO_REPO_DESTINO/dists/$SUITE/main/binary-amd64/Packages" "$TMP/servidor/" 2>"$TMP/rsync.err" ||
		! rsync -a --copy-dest="$WWW/pool/" "$KALANGO_REPO_DESTINO/pool/" "$TMP/pool/" 2>>"$TMP/rsync.err"; then
		if [ "${KALANGO_REPO_NOVO:-}" != 1 ]; then
			cat "$TMP/rsync.err" >&2
			echo "ERRO: não deu para ler o repositório do servidor. Nada foi publicado." >&2
			echo "(Servidor novo, ainda vazio: rode com KALANGO_REPO_NOVO=1.)" >&2
			exit 1
		fi
		rm -rf "$TMP/servidor" "$TMP/pool" && mkdir -p "$TMP/servidor" "$TMP/pool"
	fi

	: >"$TMP/confiaveis"
	if [ -s "$TMP/servidor/InRelease" ]; then
		gpgv --quiet --keyring "$TMP/chave.gpg" --output "$TMP/servidor/Release" "$TMP/servidor/InRelease" 2>/dev/null ||
			{ echo "ERRO: o InRelease do servidor não tem a assinatura da chave do projeto. Nada foi publicado." >&2; exit 1; }
		esperado=$(awk '/^SHA256:/{s=1;next} /^[^ ]/{s=0} s && $3=="main/binary-amd64/Packages"{print $1}' "$TMP/servidor/Release")
		[ -n "$esperado" ] && [ "$(sha256sum <"$TMP/servidor/Packages" | cut -d' ' -f1)" = "$esperado" ] ||
			{ echo "ERRO: o Packages do servidor não bate com o InRelease assinado. Nada foi publicado." >&2; exit 1; }
		# "sha256 caminho" de cada pacote assinado.
		awk '/^Filename:/{f=$2} /^SHA256:/{h=$2} /^$/{if(f&&h)print h" "f; f=h=""} END{if(f&&h)print h" "f}' \
			"$TMP/servidor/Packages" >"$TMP/confiaveis"
	fi
	# O índice local (assinado aqui na última publicação) também vale: cobre
	# o que foi publicado desta máquina e ainda não chegou ao servidor.
	if [ -s "$DIST/main/binary-amd64/Packages" ]; then
		awk '/^Filename:/{f=$2} /^SHA256:/{h=$2} /^$/{if(f&&h)print h" "f; f=h=""} END{if(f&&h)print h" "f}' \
			"$DIST/main/binary-amd64/Packages" >>"$TMP/confiaveis"
	fi

	ruins=$(cd "$TMP" && find pool -type f | sort | while read -r arq; do
		hash=$(sha256sum <"$arq" | cut -d' ' -f1)
		grep -qxF "$hash $arq" confiaveis || echo "$arq"
	done)
	if [ -n "$ruins" ]; then
		echo "ERRO: arquivos no servidor que nunca foram assinados (ou foram alterados):" >&2
		printf '  %s\n' $ruins >&2
		echo "Nada foi publicado. Confira o servidor antes de continuar." >&2
		exit 1
	fi
	# Conferidos: entram no repositório local. Um arquivo local com o mesmo
	# nome e conteúdo diferente também é recusado (não sobrescreve nada).
	(cd "$TMP" && find pool -type f) | while read -r arq; do
		if [ -e "$WWW/$arq" ]; then
			cmp -s "$TMP/$arq" "$WWW/$arq" ||
				{ echo "ERRO: $arq difere do arquivo local com o mesmo nome. Nada foi publicado." >&2; exit 1; }
		else
			mkdir -p "$(dirname "$WWW/$arq")"
			cp -p "$TMP/$arq" "$WWW/$arq"
			echo "< $arq (do servidor, assinatura conferida)"
		fi
	done
fi

for deb in "$@"; do
	nome=$(dpkg-deb -f "$deb" Package)
	letra=$(printf %s "$nome" | cut -c1)
	mkdir -p "$WWW/pool/main/$letra/$nome"
	cp "$deb" "$WWW/pool/main/$letra/$nome/"
	echo "+ $(basename "$deb")"
done

mkdir -p "$DIST/main/binary-amd64"
cd "$WWW"
dpkg-scanpackages --multiversion pool /dev/null 2>/dev/null >"$DIST/main/binary-amd64/Packages"
gzip -9nkf "$DIST/main/binary-amd64/Packages"
xz -9kf "$DIST/main/binary-amd64/Packages"

cd "$DIST"
{
	# NUNCA mude Origin/Label: o apt dos sistemas instalados recusa atualizar
	# quando eles mudam ("changed its 'Origin' value"). Era "Kalango OS" até
	# 2026-11-15; só o notebook de teste viu o valor antigo.
	echo "Origin: KalangOS"
	echo "Label: KalangOS"
	echo "Suite: $SUITE"
	echo "Codename: $SUITE"
	echo "Date: $(LC_ALL=C date -Ru)"
	echo "Architectures: amd64"
	echo "Components: main"
	echo "Description: Pacotes do KalangOS"
	echo "SHA256:"
	for f in main/binary-amd64/Packages main/binary-amd64/Packages.gz main/binary-amd64/Packages.xz; do
		printf ' %s %s %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(stat -c %s "$f")" "$f"
	done
} >Release
rm -f InRelease Release.gpg
gpg --batch --yes --local-user "$NOME" --clearsign -o InRelease Release
gpg --batch --yes --local-user "$NOME" --armor --detach-sign -o Release.gpg Release
gpg --batch --export "$NOME" >"$WWW/kalango-archive-keyring.gpg"

echo "repositório: $WWW"
grep -h '^Package\|^Version' "$DIST/main/binary-amd64/Packages" | paste - - | sort -u

if [ -n "${KALANGO_REPO_DESTINO:-}" ]; then
	# --checksum: compara o conteúdo, não só tamanho e hora (o Release tem
	# quase sempre o mesmo tamanho; publicado no mesmo segundo, não ia).
	rsync -av --checksum --delete "$WWW/" "$KALANGO_REPO_DESTINO/"
fi
