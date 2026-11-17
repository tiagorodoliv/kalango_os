#!/bin/sh
# KalangOS: tira o "resume=" do GRUB quando a swap não serve para hibernar.
#
# O Calamares põe "resume=UUID=<swap>" no GRUB sempre que cria uma partição
# swap, mesmo a pequena, que não cabe a memória e não serve para hibernar.
# Sem hibernação o parâmetro só atrapalha: se a swap um dia for apagada (para
# aproveitar o espaço), o boot fica esperando por ela. Fica só quando a swap
# tem pelo menos o tamanho da RAM (a escolha "Hibernar" do instalador).
#
# Roda dentro do sistema instalado (shellprocess@kalango do Calamares) e pode
# rodar de novo à mão: sem "resume=" no GRUB, não faz nada.
set -eu

GRUB=/etc/default/grub
[ -f "$GRUB" ] || exit 0
uuid=$(sed -n 's/.*resume=UUID=\([0-9a-fA-F-]*\).*/\1/p' "$GRUB" | head -n 1)
[ -n "$uuid" ] || exit 0

# Tamanho da swap e da memória, em kB.
dev=/dev/disk/by-uuid/$uuid
swap_kb=0
if [ -e "$dev" ]; then
	swap_kb=$(($(blockdev --getsize64 "$dev") / 1024))
fi
ram_kb=$(sed -n 's/^MemTotal: *\([0-9]*\) kB/\1/p' /proc/meminfo)

# Cabe a memória (com 5% de folga para arredondamentos): é para hibernar.
if [ "$swap_kb" -ge $((ram_kb * 95 / 100)) ]; then
	exit 0
fi

sed -i "s/ *resume=UUID=$uuid//" "$GRUB"
mkdir -p /etc/initramfs-tools/conf.d
echo "RESUME=none" >/etc/initramfs-tools/conf.d/resume
update-grub >/dev/null 2>&1 || true
echo "kalango: resume tirado do GRUB (swap de ${swap_kb} kB < memória de ${ram_kb} kB)"
