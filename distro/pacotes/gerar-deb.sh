#!/bin/sh
# Monta o pacote kalango-shell (.deb) para a ISO e para as atualizações (OTA).
#
# O pacote leva o binário do shell e os scripts do sistema que mudam junto com
# ele (/usr/local/bin, /usr/local/sbin, /usr/local/lib/kalango e o hook de
# suspensão kalango-radios), copiados de distro/config/includes.chroot. Assim o
# dpkg é dono desses arquivos e uma versão nova no repositório apt os troca
# pela página Atualizações. Os arquivos de /etc continuam só na ISO.
#
# Uso: distro/pacotes/gerar-deb.sh BINARIO PASTA_DE_SAIDA
#   BINARIO: o kalango-shell compilado contra o trixie (build.sh, testar-vm.sh)
#   KALANGO_REVISAO=N: revisão do pacote (padrão 1) → versão <Cargo.toml>-N
# Saída: PASTA_DE_SAIDA/kalango-shell_<versão>_amd64.deb
set -eu

BIN=${1:?binário do kalango-shell}
SAIDA=${2:?pasta de saída}
DISTRO=$(cd "$(dirname "$0")/.." && pwd)
ROOT=$(dirname "$DISTRO")
INC="$DISTRO/config/includes.chroot"
VERSAO=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)-${KALANGO_REVISAO:-1}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
P="$TMP/pkg"

install -D -m 0755 "$BIN" "$P/usr/local/bin/kalango-shell"
for d in usr/local/bin usr/local/sbin usr/local/lib/kalango; do
	[ -d "$INC/$d" ] || continue
	mkdir -p "$P/$d"
	for f in "$INC/$d"/*; do
		[ -f "$f" ] || continue
		case "$(basename "$f")" in .gitkeep) continue ;; esac
		cp -p "$f" "$P/$d/"
	done
done
install -D -m 0755 "$INC/usr/lib/systemd/system-sleep/kalango-radios" "$P/usr/lib/systemd/system-sleep/kalango-radios"
# Sem escrita para grupo/outros (as cópias do repositório vêm com 775/664).
chmod -R go-w "$P"

mkdir -p "$P/DEBIAN"
cat >"$P/DEBIAN/control" <<CTRL
Package: kalango-shell
Version: $VERSAO
Architecture: amd64
Maintainer: Kalango OS <repo@kalangos.easysmartcontroll.com>
Section: x11
Priority: optional
Depends: labwc, libc6
Installed-Size: $(du -sk "$P/usr" | cut -f1)
Homepage: https://github.com/tiagorodoliv/kalango_os
Description: Interface do Kalango OS (barra, dock, menu e Configurações)
 Shell gráfico do Kalango OS em Rust + Slint, com os scripts do sistema que
 acompanham cada versão (sessão, tela de login, bloqueio, energia, usuários,
 impressoras, atualizações e o conserto do Wi-Fi/Bluetooth ao acordar).
CTRL
# A sessão aberta continua com o binário antigo até entrar de novo.
cat >"$P/DEBIAN/postinst" <<'POST'
#!/bin/sh
set -e
if [ "$1" = configure ] && [ -n "${2:-}" ]; then
	echo "*** Kalango OS atualizado: reinicie (ou saia e entre) para usar a versão nova ***" >/run/reboot-required
fi
exit 0
POST
chmod 0755 "$P/DEBIAN/postinst"

mkdir -p "$SAIDA"
dpkg-deb --root-owner-group -Zxz --build "$P" "$SAIDA/kalango-shell_${VERSAO}_amd64.deb" >/dev/null
echo "$SAIDA/kalango-shell_${VERSAO}_amd64.deb"
