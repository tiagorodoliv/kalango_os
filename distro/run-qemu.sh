#!/bin/sh
# Testa a ISO do Kalango OS no QEMU (com KVM).
#
# Uso:  distro/run-qemu.sh [--uefi] [--gl] [--disco | --so-disco] [--dev pasta] [caminho.iso]
#   --uefi      dá boot por UEFI (OVMF) em vez de BIOS.
#   --gl        GPU com aceleração 3D (virgl) numa janela SDL. Mais rápido, mas
#               depende do driver de vídeo do host.
#   --disco     liga também um disco virtual de 20 GB (distro/out/disco-teste.qcow2,
#               criado na primeira vez) para testar o instalador.
#   --so-disco  dá boot pelo disco virtual, sem a ISO (sistema já instalado).
#   --dev pasta compartilha a pasta com a VM (9p, "kalango-dev"): a ISO live
#               aplica o shell e as configs dela no boot (distro/testar-vm.sh).
# Sem caminho, usa a ISO mais recente em distro/out/.
set -eu

DISTRO=$(cd "$(dirname "$0")" && pwd)
FIRMWARE=""
DISCO=""
DEV=""
ISO_ARGS="yes"
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
	--disco | --so-disco)
		IMG="$DISTRO/out/disco-teste.qcow2"
		mkdir -p "$DISTRO/out"
		[ -f "$IMG" ] || qemu-img create -f qcow2 "$IMG" 20G >/dev/null
		DISCO="-drive file=$IMG,if=virtio,format=qcow2"
		[ "$1" = "--so-disco" ] && ISO_ARGS=""
		shift
		;;
	--dev)
		DEV="-virtfs local,path=$2,mount_tag=kalango-dev,security_model=none,readonly=on"
		shift 2
		;;
	*) break ;;
	esac
done
if [ -n "$ISO_ARGS" ]; then
	ISO="${1:-$(ls -t "$DISTRO"/out/*.iso 2>/dev/null | head -n 1)}"
	[ -n "$ISO" ] || { echo "Nenhuma ISO em $DISTRO/out. Rode distro/build.sh antes." >&2; exit 1; }
	ISO_ARGS="-cdrom $ISO -boot order=dc"
fi

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
	$DISCO $DEV $ISO_ARGS
