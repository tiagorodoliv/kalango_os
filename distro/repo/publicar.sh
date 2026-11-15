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
#
# No sistema: /etc/apt/sources.list.d/kalango.sources aponta para
# https://kalangos.easysmartcontroll.com/apt (suite trixie, componente main).
set -eu

REPO=${KALANGO_REPO:-$HOME/.local/share/kalango-repo}
export GNUPGHOME="$REPO/gnupg"
NOME="repo@kalangos.easysmartcontroll.com"
WWW="$REPO/www/apt"
SUITE=trixie
DIST="$WWW/dists/$SUITE"

gpg --batch --list-secret-keys "$NOME" >/dev/null 2>&1 ||
	{ echo "Sem a chave do repositório: rode distro/repo/criar-chave.sh" >&2; exit 1; }

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
	rsync -av --delete "$WWW/" "$KALANGO_REPO_DESTINO/"
fi
