#!/bin/sh
# Gera a ISO do KalangOS.
#
# Tudo roda dentro de um chroot Debian trixie ("builder", em distro/.cache/):
#   1. o kalango-shell é compilado lá, contra a glibc do trixie
#      (um binário compilado no Ubuntu do host não roda no Debian);
#   2. a ISO é gerada lá com o live-build do Debian
#      (o live-build do Ubuntu é a versão 3.0 de 2012 e não conhece o trixie).
#
# Uso:  distro/build.sh             (pede sudo; a primeira vez baixa ~1 GB)
#       distro/build.sh --so-shell  (só compila o kalango-shell e monta o .deb)
# Saída: distro/out/kalango-os-<versão>.iso (versão lida do Cargo.toml)
#
# Variáveis opcionais: KALANGO_MIRROR (espelho Debian, ex.: http://ftp.br.debian.org/debian/) e
# KALANGO_REVISAO (revisão do pacote kalango-shell da ISO; use a mesma publicada no apt)
set -eu

# Reexecuta como root mantendo o espelho e a revisão do pacote escolhidos.
if [ "$(id -u)" -ne 0 ]; then
	exec sudo KALANGO_MIRROR="${KALANGO_MIRROR:-}" KALANGO_REVISAO="${KALANGO_REVISAO:-1}" "$0" "$@"
fi

DISTRO=$(cd "$(dirname "$0")" && pwd)
ROOT=$(dirname "$DISTRO")
BUILDER="$DISTRO/.cache/builder"
OUT="$DISTRO/out"
# Versão do sistema = versão do pacote do shell (campo version do [package] no Cargo.toml).
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n 1)
[ -n "$VERSION" ] || { echo "Não consegui ler a versão do Cargo.toml." >&2; exit 1; }
ISO_NAME="kalango-os-$VERSION.iso"
MIRROR="${KALANGO_MIRROR:-http://deb.debian.org/debian/}"
SO_SHELL=0
[ "${1:-}" = "--so-shell" ] && SO_SHELL=1

# Dono dos arquivos de saída: quem chamou o sudo.
OWNER="${SUDO_UID:-0}:${SUDO_GID:-0}"

# Roda um comando dentro do builder com ambiente limpo.
no_builder() {
	env -i HOME=/root LANG=C.UTF-8 TERM="${TERM:-xterm}" \
		PATH=/root/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
		KALANGO_MIRROR="$MIRROR" \
		chroot "$BUILDER" /bin/sh -ec "$1"
}

# --- 1. Cria o builder (só na primeira vez) ---------------------------------
if [ ! -f "$BUILDER/.pronto" ]; then
	echo ">> Criando chroot Debian trixie em $BUILDER"
	mkdir -p "$BUILDER"
	debootstrap --variant=minbase \
		--keyring=/usr/share/keyrings/debian-archive-keyring.gpg \
		trixie "$BUILDER" "$MIRROR"
fi

# Monta /proc, /sys e /dev (o live-build cria outro chroot lá dentro).
# make-rslave impede que desmontar no builder desmonte algo do host.
montar() {
	mountpoint -q "$BUILDER/$1" && return 0
	mount --rbind "/$1" "$BUILDER/$1"
	mount --make-rslave "$BUILDER/$1"
}
# Desmonta o que o live-build deixou montado no chroot da ISO (sobra quando ele falha
# no meio). Do mais fundo para o mais raso, para não deixar nada preso.
desmontar_iso() {
	findmnt -rn -o TARGET | grep "^$BUILDER/work/" | sort -r | while read -r m; do
		umount -l "$m" || true
	done
}
desmontar() {
	desmontar_iso
	for d in dev sys proc; do
		mountpoint -q "$BUILDER/$d" && umount -R -l "$BUILDER/$d" || true
	done
}
trap desmontar EXIT INT TERM
montar proc
montar sys
montar dev
cp -L /etc/resolv.conf "$BUILDER/etc/resolv.conf"

if [ ! -f "$BUILDER/.pronto" ]; then
	echo ">> Instalando live-build e dependências de compilação no builder"
	no_builder "
		apt-get update
		DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
			live-build debootstrap ca-certificates curl rsync apt-utils \
			build-essential pkg-config \
			libfontconfig-dev libxkbcommon-dev libwayland-dev \
			libudev-dev libinput-dev libgbm-dev libdrm-dev
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
			| sh -s -- -y --profile minimal --no-modify-path
	"
	touch "$BUILDER/.pronto"
fi
# Builder antigo (de antes da lista kalango.list.binary): o live-build precisa do
# apt-ftparchive (apt-utils) para montar o repositório de pacotes dentro da ISO.
if [ ! -x "$BUILDER/usr/bin/apt-ftparchive" ]; then
	echo ">> Instalando apt-utils no builder"
	no_builder "DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends apt-utils"
fi

# --- 2. Compila o kalango-shell ---------------------------------------------
echo ">> Compilando kalango-shell (release) no builder"
mkdir -p "$BUILDER/work/shell"
rsync -a --delete \
	--include='/Cargo.toml' --include='/Cargo.lock' --include='/build.rs' \
	--include='/src/***' --include='/ui/***' --include='/lang/***' --exclude='*' \
	"$ROOT/" "$BUILDER/work/shell/"
no_builder "cd /work/shell && cargo build --release --locked"

if [ "$SO_SHELL" -eq 1 ]; then
	mkdir -p "$OUT"
	cp "$BUILDER/work/shell/target/release/kalango_os" "$OUT/kalango-shell"
	# Pacote para atualizar sem ISO nova: distro/repo/publicar.sh distro/out/kalango-shell_*.deb
	rm -f "$OUT"/kalango-shell_*.deb
	sh "$DISTRO/pacotes/gerar-deb.sh" "$OUT/kalango-shell" "$OUT"
	chown "$OWNER" "$OUT" "$OUT/kalango-shell" "$OUT"/kalango-shell_*.deb
	echo ">> Pronto: $OUT/kalango-shell e o pacote .deb"
	exit 0
fi

# --- 3. Gera a ISO ----------------------------------------------------------
echo ">> Gerando a ISO com live-build"
mkdir -p "$BUILDER/work/iso"
desmontar_iso
no_builder "cd /work/iso && { [ -x auto/clean ] && lb clean || true; }"
rsync -a --delete "$DISTRO/auto/" "$BUILDER/work/iso/auto/"
rsync -a --delete "$DISTRO/config/" "$BUILDER/work/iso/config/"
# O shell vai como pacote (kalango-shell, distro/pacotes/gerar-deb.sh): o dpkg fica
# dono dos arquivos e as versões novas chegam pelo repositório apt (OTA). Uma
# cópia do .deb fica em distro/out para publicar com distro/repo/publicar.sh.
mkdir -p "$BUILDER/work/iso/config/packages.chroot" "$OUT"
rm -f "$BUILDER/work/iso/config/packages.chroot"/kalango-shell_*.deb "$OUT"/kalango-shell_*.deb
sh "$DISTRO/pacotes/gerar-deb.sh" "$BUILDER/work/shell/target/release/kalango_os" \
	"$BUILDER/work/iso/config/packages.chroot"
cp "$BUILDER/work/iso/config/packages.chroot"/kalango-shell_*.deb "$OUT/"
chown "$OWNER" "$OUT"/kalango-shell_*.deb
# A mesma versão já publicada no apt com outro conteúdo (cada compilação gera um
# binário diferente) quebra o lb build lá no fim: o apt da ISO vê as duas e
# tenta "voltar" de versão. Confere agora, antes da parte demorada.
DEB=$(ls "$OUT"/kalango-shell_*.deb)
VERSAO_DEB=$(dpkg-deb -f "$DEB" Version)
SHA_DEB=$(sha256sum "$DEB" | cut -d' ' -f1)
SHA_PUB=$(no_builder "curl -fsSL https://kalangos.org/apt/dists/trixie/main/binary-amd64/Packages" 2>/dev/null |
	awk -v v="$VERSAO_DEB" '/^Version:/ { ver = $2 } /^SHA256:/ && ver == v { print $2 }' | head -n 1 || true)
if [ -n "$SHA_PUB" ] && [ "$SHA_PUB" != "$SHA_DEB" ]; then
	REV=${VERSAO_DEB##*-}
	echo "kalango-shell $VERSAO_DEB já está publicado no apt com outro conteúdo." >&2
	echo "Gere a ISO com a revisão seguinte e publique o .deb de $OUT junto com ela:" >&2
	echo "  KALANGO_REVISAO=$((REV + 1)) distro/build.sh" >&2
	exit 1
fi
rm -f "$BUILDER/work/iso/config/includes.chroot/usr/local/bin/.gitkeep"
# Papéis de parede (assets/wallpapers/16x9, gerados por gerar.py): src/shell/wallpaper.rs.
install -d "$BUILDER/work/iso/config/includes.chroot/usr/share/backgrounds/kalango"
cp "$ROOT"/assets/wallpapers/16x9/*.png "$BUILDER/work/iso/config/includes.chroot/usr/share/backgrounds/kalango/"
# Ícones (Flat Remix + pastas por cor de destaque), baixados uma vez para o cache.
sh "$DISTRO/icones/flat-remix.sh" "$BUILDER/work/iso/config/includes.chroot/usr/share/icons" "$DISTRO/.cache"
# Temas extras da Aparência (Colloid e Fluent), também com cache.
sh "$DISTRO/icones/temas-extras.sh" "$BUILDER/work/iso/config/includes.chroot/usr/share/icons" "$DISTRO/.cache"
# Botão "Trocar de usuário" da tela de bloqueio (gtklock-powerbar-module, que não
# está no Debian 13): baixado uma vez para o cache e compilado no builder.
POWERBAR=$(sh "$DISTRO/gtklock/powerbar.sh" baixar "$DISTRO/.cache")
cp "$POWERBAR" "$BUILDER/work/powerbar.tar.gz"
cp "$DISTRO/gtklock/powerbar.sh" "$BUILDER/work/powerbar.sh"
no_builder "sh /work/powerbar.sh compilar /work/powerbar.tar.gz /work/iso/config/includes.chroot/usr/lib/x86_64-linux-gnu/gtklock"
no_builder "cd /work/iso && lb config"
# O live-build às vezes falha ao desmontar o /proc do chroot ("target is busy"): um processo
# do apt ainda está saindo. Ele retoma de onde parou, então basta rodar de novo. As
# montagens ficam como estão, porque o lb confia nos próprios marcadores para retomar.
tentativa=1
until no_builder "cd /work/iso && lb build"; do
	if [ "$tentativa" -ge 3 ]; then
		echo "lb build falhou $tentativa vezes; veja $BUILDER/work/iso/build.log" >&2
		exit 1
	fi
	tentativa=$((tentativa + 1))
	echo ">> lb build falhou; tentando de novo em 10 s ($tentativa/3)"
	sleep 10
done

mkdir -p "$OUT"
# O live-build gera <image-name>-amd64.hybrid.iso (auto/config); renomeia com a versão.
cp "$BUILDER/work/iso/kalango-os-amd64.hybrid.iso" "$OUT/$ISO_NAME"
cp "$BUILDER/work/iso/build.log" "$OUT/"
chown -R "$OWNER" "$OUT"
echo ">> Pronto:"
ls -lh "$OUT/$ISO_NAME"
