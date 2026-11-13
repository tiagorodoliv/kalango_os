#!/bin/sh
# Liga a VM usada para gravar o vídeo de apresentação (sem janela: a tela vai
# por VNC e o controle por QMP, em sockets dentro de video/.cache).
#
# Usa a ISO de distro/out e, se existir, o shell de desenvolvimento preparado
# por `distro/testar-vm.sh --so-preparar` (aplicado no boot pelo
# kalango-dev.service). Com --ingles, força inglês e teclado us mesmo numa
# ISO antiga (que iniciava em português).
#
# Uso: video/vm.sh [--ingles]   |   video/vm.sh --desligar
set -eu

VIDEO=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$VIDEO")
CACHE="$VIDEO/.cache"
mkdir -p "$CACHE"

if [ -f "$CACHE/qemu.pid" ]; then
	kill "$(cat "$CACHE/qemu.pid")" 2>/dev/null || true
	sleep 1
	rm -f "$CACHE/qemu.pid"
fi
[ "${1:-}" = "--desligar" ] && exit 0
rm -f "$CACHE/vnc.sock" "$CACHE/qmp.sock"

ISO=$(ls -t "$ROOT"/distro/out/kalango-os-*.iso | head -1)
DEV=""
if [ -f "$ROOT/target/vm-dev/kalango-shell" ]; then
	rm -rf "$CACHE/dev"
	mkdir -p "$CACHE/dev"
	cp "$ROOT/target/vm-dev/kalango-shell" "$CACHE/dev/"
	cp -r "$ROOT/target/vm-dev/raiz" "$CACHE/dev/raiz"
	if [ "${1:-}" = "--ingles" ]; then
		R="$CACHE/dev/raiz"
		mkdir -p "$R/etc/default" "$R/etc/xdg/labwc"
		printf 'LANG=en_US.UTF-8\n' >"$R/etc/default/locale"
		printf 'XKBMODEL="pc105"\nXKBLAYOUT="us"\nXKBVARIANT=""\nXKBOPTIONS=""\n' >"$R/etc/default/keyboard"
		# A ISO antiga tinha XKB_DEFAULT_LAYOUT=br aqui; vazio, vale o regiao.sh.
		printf '# (vazio: as variáveis da sessão vêm do kalango-session)\n' >"$R/etc/xdg/labwc/environment"
	fi
	DEV="-virtfs local,path=$CACHE/dev,mount_tag=kalango-dev,security_model=none,readonly=on"
fi

# shellcheck disable=SC2086
qemu-system-x86_64 -enable-kvm -machine q35,vmport=off -cpu host -smp 4 -m 6144 \
	-device virtio-vga,xres=1920,yres=1080 -display none \
	-vnc "unix:$CACHE/vnc.sock" -qmp "unix:$CACHE/qmp.sock,server=on,wait=off" \
	-device qemu-xhci -device usb-tablet \
	-audiodev none,id=som -device intel-hda -device hda-duplex,audiodev=som \
	-nic user,model=virtio-net-pci \
	$DEV -cdrom "$ISO" -boot d \
	-daemonize -pidfile "$CACHE/qemu.pid"
echo ">> VM ligada ($ISO). Tela: $CACHE/vnc.sock; controle: $CACHE/qmp.sock"
