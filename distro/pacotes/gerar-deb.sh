#!/bin/sh
# Monta o pacote kalango-shell (.deb) para a ISO e para as atualizações (OTA).
#
# O pacote leva o binário do shell e os scripts do sistema que mudam junto com
# ele (/usr/local/bin, /usr/local/sbin, /usr/local/lib/kalango e o hook de
# suspensão kalango-radios), copiados de distro/config/includes.chroot. Assim o
# dpkg é dono desses arquivos e uma versão nova no repositório apt os troca
# pela página Atualizações. Os arquivos de /etc continuam só na ISO.
#
# O terminal também vai no pacote, para quem só atualiza pelo apt (instalado
# por uma ISO antiga): os temas do foot (/usr/share/kalango/terminal) e o
# ~/.zshrc no estilo do Kali (/usr/share/kalango/zshrc). O postinst põe o
# .zshrc no /etc/skel, deixa o zsh como shell de quem for criado depois e,
# UMA vez por máquina, passa para o zsh quem ainda está no bash (quem voltar
# para o bash depois não é forçado de novo). Também liga o PAM do KalangOS
# (pam-kalango.sh: senhas de qualquer tamanho e PIN) e o desfaz no prerm.
#
# Uso: distro/pacotes/gerar-deb.sh BINARIO PASTA_DE_SAIDA
#   BINARIO: o kalango-shell compilado contra o trixie (build.sh, testar-vm.sh)
#   KALANGO_REVISAO=N: revisão do pacote (padrão 1) → versão <Cargo.toml>-N
# Também leva as políticas do polkit (org.kalango.*) e o limite de 5 s para o
# xdg-document-portal parar ao sair (antes a tela ficava 90 s parada).
#
# O preinst guarda o binário em uso em /var/lib/kalango/shell-anterior, para o
# supervisor (kalango-shell-sessao) voltar a ele se a versão nova não abrir; o
# postinst põe o supervisor no autostart do labwc e não pede mais para reiniciar.
#
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
# Políticas do polkit dos scripts acima (kalango-integridade, -atualizar...):
# quem só atualiza pelo apt também recebe as novas.
mkdir -p "$P/usr/share/polkit-1/actions"
cp -p "$INC/usr/share/polkit-1/actions/"org.kalango.*.policy "$P/usr/share/polkit-1/actions/"
# Captura de tela no menu de apps ("Todos os apps"; tecla Print).
install -D -m 0644 "$INC/usr/share/applications/kalango-captura.desktop" \
	"$P/usr/share/applications/kalango-captura.desktop"
# Sair/reiniciar sem esperar 90 s pelo portal de documentos do Flatpak.
install -D -m 0644 "$INC/usr/lib/systemd/user/xdg-document-portal.service.d/50-kalango.conf" \
	"$P/usr/lib/systemd/user/xdg-document-portal.service.d/50-kalango.conf"
# Terminal: temas do foot e o .zshrc do Kali (o /etc/skel é do postinst, para
# não virar conffile nem brigar com a cópia que a ISO já deixa lá).
mkdir -p "$P/usr/share/kalango/terminal"
cp -p "$INC/usr/share/kalango/terminal/"*.ini "$P/usr/share/kalango/terminal/"
install -D -m 0644 "$INC/etc/skel/.zshrc" "$P/usr/share/kalango/zshrc"
# Sem escrita para grupo/outros (as cópias do repositório vêm com 775/664).
chmod -R go-w "$P"

mkdir -p "$P/DEBIAN"
cat >"$P/DEBIAN/control" <<CTRL
Package: kalango-shell
Version: $VERSAO
Architecture: amd64
Maintainer: KalangOS <repo@kalangos.org>
Section: x11
Priority: optional
Depends: labwc, libc6, foot, zsh, zsh-autosuggestions, zsh-syntax-highlighting, fonts-droid-fallback, grim, wf-recorder, wl-clipboard
Recommends: sound-theme-freedesktop
Installed-Size: $(du -sk "$P/usr" | cut -f1)
Homepage: https://github.com/tiagorodoliv/kalango_os
Description: Interface do KalangOS (barra, dock, menu e Configurações)
 Shell gráfico do KalangOS em Rust + Slint, com os scripts do sistema que
 acompanham cada versão (sessão, tela de login, bloqueio, energia, usuários,
 impressoras, atualizações e o conserto do Wi-Fi/Bluetooth ao acordar).
CTRL
# A sessão aberta continua com o binário antigo até entrar de novo.
cat >"$P/DEBIAN/postinst" <<'POST'
#!/bin/sh
set -e

if [ "$1" = configure ]; then
	# Senhas de qualquer tamanho e PIN de login/desbloqueio (PAM).
	sh /usr/local/lib/kalango/pam-kalango.sh ligar

	ZSHRC=/usr/share/kalango/zshrc
	# /etc/skel/.zshrc: só se não existe ou ainda é um dos nossos (primeira
	# linha "# ~/.zshrc do Kalang..."); um feito pelo administrador fica.
	if [ ! -f /etc/skel/.zshrc ] || head -n 1 /etc/skel/.zshrc | grep -q '^# ~/.zshrc do Kalang'; then
		install -m 0644 "$ZSHRC" /etc/skel/.zshrc
	fi
	# Usuários criados depois (adduser, useradd) já nascem no zsh (como o hook 0140 da ISO).
	if [ -f /etc/adduser.conf ]; then
		sed -i 's|^#\?DSHELL=.*|DSHELL=/bin/zsh|' /etc/adduser.conf
	fi
	if grep -q '^SHELL=' /etc/default/useradd 2>/dev/null; then
		sed -i 's|^SHELL=.*|SHELL=/bin/zsh|' /etc/default/useradd
	else
		echo 'SHELL=/bin/zsh' >>/etc/default/useradd
	fi
	# Uma vez por máquina: pessoas (UID 1000–59999) ganham o .zshrc que falta e
	# quem está no bash passa para o zsh. A marca impede repetir a troca.
	MARCA=/var/lib/kalango/terminal-zsh
	if [ ! -e "$MARCA" ] && [ -x /bin/zsh ]; then
		getent passwd | while IFS=: read -r nome _ uid gid _ casa shell; do
			[ "$uid" -ge 1000 ] && [ "$uid" -lt 60000 ] || continue
			[ -d "$casa" ] || continue
			if [ ! -e "$casa/.zshrc" ]; then
				install -m 0644 -o "$uid" -g "$gid" "$ZSHRC" "$casa/.zshrc"
			fi
			case "$shell" in
				/bin/bash | /usr/bin/bash) usermod -s /bin/zsh "$nome" ;;
			esac
		done
		mkdir -p "${MARCA%/*}"
		date -Iseconds >"$MARCA"
	fi
fi

if [ "$1" = configure ]; then
	# Repositório no domínio do projeto (2026-11-17): sistemas instalados antes
	# apontavam para kalangos.easysmartcontroll.com, que continua servindo os
	# mesmos arquivos. Só troca a linha URIs do nosso arquivo; um feito à mão
	# com outro endereço fica como está.
	FONTE=/etc/apt/sources.list.d/kalango.sources
	if [ -f "$FONTE" ]; then
		sed -i 's|^URIs: https://kalangos\.easysmartcontroll\.com/apt$|URIs: https://kalangos.org/apt|' "$FONTE"
	fi
	# os-release (protegido do base-files pelo hook 0050): o site novo.
	if [ -f /usr/lib/os-release ]; then
		sed -i 's|^HOME_URL="https://kalangos\.easysmartcontroll\.com/"$|HOME_URL="https://kalangos.org/"|' /usr/lib/os-release
	fi
fi

if [ "$1" = configure ]; then
	# A área de trabalho sob o supervisor (kalango-shell-sessao: reabre se
	# cair, volta para a versão anterior se a nova não abrir). O autostart do
	# labwc é da ISO; só a linha exata do padrão é trocada.
	AUTO=/etc/xdg/labwc/autostart
	if [ -f "$AUTO" ]; then
		sed -i 's|^systemd-cat -t kalango-shell kalango-shell &$|systemd-cat -t kalango-shell kalango-shell-sessao \&|' "$AUTO"
	fi
fi
# Sem pedido de reiniciar: a área de trabalho percebe a versão nova e oferece
# recarregar a interface sem fechar os apps (src/shell/update.rs).
exit 0
POST
chmod 0755 "$P/DEBIAN/postinst"
# Antes de trocar: guarda o binário em uso como versão anterior, que o
# supervisor (kalango-shell-sessao) usa se a versão nova não abrir.
cat >"$P/DEBIAN/preinst" <<'PRE'
#!/bin/sh
set -e
if [ "$1" = upgrade ] && [ -x /usr/local/bin/kalango-shell ]; then
	ANT=/var/lib/kalango/shell-anterior
	mkdir -p "$ANT"
	cp -p /usr/local/bin/kalango-shell "$ANT/kalango-shell.novo"
	mv "$ANT/kalango-shell.novo" "$ANT/kalango-shell"
	echo "${2:-}" >"$ANT/versao"
fi
exit 0
PRE
chmod 0755 "$P/DEBIAN/preinst"
# Ao remover de vez (purge), apaga a versão anterior guardada.
cat >"$P/DEBIAN/postrm" <<'POSTRM'
#!/bin/sh
set -e
if [ "$1" = purge ]; then
	rm -rf /var/lib/kalango/shell-anterior
fi
exit 0
POSTRM
chmod 0755 "$P/DEBIAN/postrm"
# Ao remover o pacote, o PAM volta ao do Debian (o script ainda existe no prerm).
cat >"$P/DEBIAN/prerm" <<'PRERM'
#!/bin/sh
set -e
if [ "$1" = remove ] && [ -x /usr/local/lib/kalango/pam-kalango.sh ]; then
	sh /usr/local/lib/kalango/pam-kalango.sh desligar
fi
exit 0
PRERM
chmod 0755 "$P/DEBIAN/prerm"

mkdir -p "$SAIDA"
dpkg-deb --root-owner-group -Zxz --build "$P" "$SAIDA/kalango-shell_${VERSAO}_amd64.deb" >/dev/null
echo "$SAIDA/kalango-shell_${VERSAO}_amd64.deb"
