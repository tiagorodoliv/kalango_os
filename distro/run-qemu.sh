#!/bin/sh
# Testa a ISO do Kalango OS no QEMU (com KVM).
#
# Uso:  distro/run-qemu.sh [--uefi] [--gl] [caminho.iso]
#   --uefi  dá boot por UEFI (OVMF) em vez de BIOS.
#   --gl    GPU com aceleração 3D (virgl) numa janela SDL. Mais rápido, mas
#           depende do driver de vídeo do host.
# Sem caminho, usa a ISO mais recente em distro/out/.
set -eu

DISTRO=$(cd "$(dirname "$0")" && pwd)
FIRMWARE=""
# Padrão: janela GTK sem OpenGL. É o caminho mais testado do QEMU. Com
# "gtk,gl=on" num desktop Wayland (GNOME) o mouse sai invertido e a imagem
# fica estranha. Sem 3D o sistema desenha na CPU (llvmpipe), o que basta
# para o shell em 1440x900.
VIDEO="-device virtio-vga,xres=1440,yres=900 -display gtk,zoom-to-fit=off"
while [ $# -gt 0 ]; do
	case "$1" in
	--uefi)
		FIRMWARE="-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd"
		shift
		;;
	--gl)
		VIDEO="-device virtio-vga-gl,xres=1440,yres=900 -display sdl,gl=on"
		shift
		;;
	*) break ;;
	esac
done
ISO="${1:-$(ls -t "$DISTRO"/out/*.iso 2>/dev/null | head -n 1)}"
[ -n "$ISO" ] || { echo "Nenhuma ISO em $DISTRO/out. Rode distro/build.sh antes." >&2; exit 1; }

# xres/yres: 1440x900, a resolução de referência do layout.
# zoom-to-fit=off: a janela do QEMU mostra a tela 1:1, sem esticar.
# usb-tablet: o mouse acompanha o ponteiro do host sem precisar capturar.
# vmport=off: sem o "VMware VMMouse", que disputaria o ponteiro com o tablet.
# shellcheck disable=SC2086
exec qemu-system-x86_64 \
	-enable-kvm -machine q35,vmport=off -cpu host -smp 4 -m 4096 \
	$FIRMWARE \
	$VIDEO \
	-device qemu-xhci -device usb-tablet \
	-nic user,model=virtio-net-pci \
	-cdrom "$ISO" -boot d
