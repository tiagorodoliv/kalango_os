#!/bin/sh
# Testa o shell e as configs da distro numa VM, sem gerar a ISO e sem sudo.
#
#   1. compila o kalango-shell contra o Debian trixie (o chroot de build que
#      o distro/build.sh criou), com o perfil rápido "vm" do Cargo.toml;
#   2. junta em target/vm-dev/ o binário e distro/config/includes.chroot;
#   3. liga a última ISO no QEMU compartilhando essa pasta: no boot, o
#      kalango-dev.service da ISO aplica tudo antes da tela de login.
#
# Uso:  distro/testar-vm.sh [--tudo] [--so-preparar] [opções do run-qemu.sh]
#   --tudo         leva também os papéis de parede e os ícones (mais lento;
#                  só precisa quando eles mudam, a ISO já traz os dela).
#   --so-preparar  só compila e prepara a pasta, com a VM já ligada. Depois,
#                  dentro da VM: sudo kalango-dev-aplicar --reiniciar
#
# Pacotes novos (package-lists) e o instalador precisam da ISO de verdade
# (distro/build.sh). A ISO precisa ter o kalango-dev.service (builds a partir
# de 2026-11-12).
set -eu

DISTRO=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$DISTRO")
BUILDER="$DISTRO/.cache/builder"
DEV="$ROOT/target/vm-dev"
ALVO="$ROOT/target/trixie"
TUDO=0
SO_PREPARAR=0
while [ $# -gt 0 ]; do
	case "$1" in
	--tudo) TUDO=1; shift ;;
	--so-preparar) SO_PREPARAR=1; shift ;;
	*) break ;;
	esac
done

[ -x "$BUILDER/usr/bin/cc" ] || {
	echo "Falta o Debian de build ($BUILDER). Rode distro/build.sh uma vez antes." >&2
	exit 1
}
command -v bwrap >/dev/null || { echo "Falta o bubblewrap: sudo apt install bubblewrap" >&2; exit 1; }
SYSROOT=$("$HOME/.cargo/bin/rustc" --print sysroot)

# --- 1. Compila contra o trixie: o chroot de build como raiz (só leitura) e
# o Rust do computador. Nada é instalado nem precisa de root.
echo ">> Compilando o kalango-shell (trixie, perfil vm)"
mkdir -p "$ALVO" "$HOME/.cargo/registry"
bwrap --ro-bind "$BUILDER" / --dev /dev --proc /proc --tmpfs /tmp \
	--ro-bind /etc/resolv.conf /etc/resolv.conf \
	--ro-bind "$SYSROOT" /mnt \
	--tmpfs /srv \
	--ro-bind "$ROOT" /srv/src \
	--bind "$ALVO" /srv/target \
	--bind "$HOME/.cargo/registry" /srv/cargo/registry \
	--setenv HOME /tmp --setenv CARGO_HOME /srv/cargo --setenv CARGO_TARGET_DIR /srv/target \
	--setenv PATH /mnt/bin:/usr/bin:/bin \
	--chdir /srv/src cargo build --profile vm --locked

# --- 2. Pasta compartilhada com a VM.
echo ">> Preparando $DEV"
rm -rf "$DEV"
mkdir -p "$DEV/raiz"
cp "$ALVO/vm/kalango_os" "$DEV/kalango-shell"
cp -r "$DISTRO/config/includes.chroot/." "$DEV/raiz/"
if [ "$TUDO" -eq 1 ]; then
	install -d "$DEV/raiz/usr/share/backgrounds/kalango"
	cp "$ROOT"/assets/wallpapers/16x9/*.png "$DEV/raiz/usr/share/backgrounds/kalango/"
	# O pacote do Flat Remix fica no cache do build (ou num cache próprio).
	CACHE="$DISTRO/.cache"
	[ -w "$CACHE" ] || [ -f "$CACHE"/flat-remix-*.tar.gz ] || CACHE="$ROOT/target/cache"
	sh "$DISTRO/icones/flat-remix.sh" "$DEV/raiz/usr/share/icons" "$CACHE"
	sh "$DISTRO/icones/temas-extras.sh" "$DEV/raiz/usr/share/icons" "$CACHE"
fi

if [ "$SO_PREPARAR" -eq 1 ]; then
	echo ">> Pronto. Na VM: sudo kalango-dev-aplicar --reiniciar"
	exit 0
fi

# --- 3. VM com a pasta compartilhada.
exec "$DISTRO/run-qemu.sh" --dev "$DEV" "$@"
