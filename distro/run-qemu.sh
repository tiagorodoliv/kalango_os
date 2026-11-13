#!/bin/sh
# Testa a ISO do Kalango OS no QEMU (com KVM).
#
# Uso:  distro/run-qemu.sh [--uefi | --secureboot] [--gl] [--disco | --so-disco] [--pendrive]
#                          [--dev pasta] [caminho.iso]
#   --uefi      dá boot por UEFI (OVMF) em vez de BIOS.
#   --secureboot UEFI com Secure Boot ligado (chaves da Microsoft, como num PC
#               com Windows). As variáveis da UEFI (entradas de boot) ficam em
#               distro/out/ovmf-vars-secureboot.fd, para o --so-disco depois.
#   --gl        GPU com aceleração 3D (virgl) numa janela SDL. Mais rápido, mas
#               depende do driver de vídeo do host.
#   --disco     liga também um disco virtual de 20 GB (distro/out/disco-teste.qcow2,
#               criado na primeira vez) para testar o instalador.
#   --so-disco  dá boot pelo disco virtual, sem a ISO (sistema já instalado).
#   --pendrive  liga um pendrive USB de 1 GB (distro/out/pendrive-teste.img, FAT32)
#               que grava a 2 MB/s, lento como um pendrive barato: para testar a
#               cópia que não pode terminar antes da gravação (src/usb.rs).
#   --dev pasta compartilha a pasta com a VM (9p, "kalango-dev"): a ISO live
#               aplica o shell e as configs dela no boot (distro/testar-vm.sh).
# Sem caminho, usa a ISO mais recente em distro/out/.
set -eu

DISTRO=$(cd "$(dirname "$0")" && pwd)
FIRMWARE=""
MACHINE="q35,vmport=off"
DISCO=""
DEV=""
PENDRIVE=""
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
	--secureboot)
		VARS="$DISTRO/out/ovmf-vars-secureboot.fd"
		mkdir -p "$DISTRO/out"
		[ -f "$VARS" ] || cp /usr/share/OVMF/OVMF_VARS_4M.ms.fd "$VARS"
		# O firmware com Secure Boot exige SMM (modo de gerenciamento do sistema).
		MACHINE="q35,vmport=off,smm=on"
		FIRMWARE="-global driver=cfi.pflash01,property=secure,value=on
			-drive if=pflash,format=raw,unit=0,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.secboot.fd
			-drive if=pflash,format=raw,unit=1,file=$VARS"
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
	--pendrive)
		PEN="$DISTRO/out/pendrive-teste.img"
		mkdir -p "$DISTRO/out"
		if [ ! -f "$PEN" ]; then
			truncate -s 1G "$PEN"
			# Uma partição FAT32 ocupando o disco, como vem de fábrica.
			echo 'type=c' | sfdisk -q "$PEN"
			mkfs.vfat -F 32 -n PENDRIVE --offset 2048 "$PEN" >/dev/null
		fi
		PENDRIVE="-drive if=none,id=pendrive,format=raw,file=$PEN,throttling.bps-write=2000000
			-device usb-storage,drive=pendrive,removable=on"
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
	-enable-kvm -machine "$MACHINE" -cpu host -smp 4 -m 4096 \
	$FIRMWARE \
	$VIDEO \
	-device qemu-xhci -device usb-tablet \
	-nic user,model=virtio-net-pci \
	$DISCO $PENDRIVE $DEV $ISO_ARGS
