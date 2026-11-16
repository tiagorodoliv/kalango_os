#!/bin/sh
# Restaura no KalangOS (ou em outro Linux) o backup feito por fazer-backup.sh.
#
# Uso: ./restaurar-backup.sh ARQUIVO.tar.zst[.gpg]
#
# - Devolve os arquivos à pasta pessoal. Não sobrescreve o que é do
#   KalangOS (tema, painel, zsh, configurações do GNOME do Ubuntu).
# - Ajusta as permissões das chaves (SSH, GPG).
# - Recoloca as conexões salvas (Wi-Fi e VPN) no NetworkManager (pede sudo).
# - Reinstala as extensões do VS Code (se o VS Code já estiver instalado).
# - Deixa as listas de programas do sistema antigo em ~/Backup-sistema-antigo.
set -eu

ARQ=${1:-}
[ -f "$ARQ" ] || { echo "Uso: $0 ARQUIVO.tar.zst[.gpg]"; exit 1; }
[ "$(id -u)" -ne 0 ] || { echo "Rode como o seu usuário (o sudo é pedido só para a parte de rede)."; exit 1; }
command -v zstd >/dev/null || { echo "Falta o zstd: sudo apt install zstd"; exit 1; }

if [ -f "$ARQ.sha256" ]; then
	echo ">> Conferindo a integridade do arquivo"
	(cd "$(dirname "$ARQ")" && sha256sum -c "$(basename "$ARQ").sha256") || { echo "!! Arquivo corrompido."; exit 1; }
fi

ler() {
	case "$ARQ" in
		*.gpg) gpg -d --pinentry-mode loopback -q "$ARQ" ;;
		*) cat "$ARQ" ;;
	esac
}

# Do sistema antigo, não traz: tema/painel/configurações do GNOME (brigariam
# com os do KalangOS) e o .zshrc/.bashrc novos do KalangOS ficam.
cd "$HOME"
echo ">> Extraindo na pasta pessoal (pode levar vários minutos)"
ler | zstd -dq | tar -x --keep-newer-files \
	--exclude=./.config/dconf --exclude=./.config/labwc --exclude=./.config/kalango \
	--exclude=./.config/gtk-3.0 --exclude=./.config/gtk-4.0 --exclude=./.config/autostart \
	--exclude=./.config/monitors.xml --exclude=./.local/share/gnome-shell \
	--exclude=./.zshrc \
	--warning=no-ignore-newer 2>&1 | grep -v 'is newer or same age' || true

echo ">> Permissões das chaves"
[ -d .ssh ] && { chmod 700 .ssh; find .ssh -type f -exec chmod 600 {} +; find .ssh -name '*.pub' -exec chmod 644 {} +; }
[ -d .gnupg ] && { chmod 700 .gnupg; find .gnupg -type d -exec chmod 700 {} +; find .gnupg -type f -exec chmod 600 {} +; }
[ -d .local/share/kalango-repo/gnupg ] && chmod -R go-rwx .local/share/kalango-repo/gnupg

EXTRA="$HOME/.kalango-backup-extra"
ANTIGO="$HOME/Backup-sistema-antigo"
if [ -d "$EXTRA" ]; then
	mkdir -p "$ANTIGO"
	if [ -d "$EXTRA/rede" ] && ls "$EXTRA/rede"/* >/dev/null 2>&1; then
		echo ">> Conexões de rede (Wi-Fi, VPN): pede a senha do sudo"
		for f in "$EXTRA/rede"/*; do
			# Conexões presas ao nome da placa do computador antigo ficam de fora.
			case "$(basename "$f")" in netplan-*) continue ;; esac
			sudo install -m 600 -o root -g root "$f" "/etc/NetworkManager/system-connections/$(basename "$f")"
			echo "   $(basename "$f" .nmconnection)"
		done
		sudo nmcli connection reload || true
	fi
	if [ -d "$EXTRA/wireguard" ]; then
		sudo install -d -m 700 /etc/wireguard
		sudo cp -a "$EXTRA/wireguard/." /etc/wireguard/
	fi
	if command -v code >/dev/null && [ -s "$EXTRA/vscode-extensoes.txt" ]; then
		echo ">> Extensões do VS Code"
		xargs -r -n1 code --install-extension <"$EXTRA/vscode-extensoes.txt" >/dev/null 2>&1 || true
	fi
	cp "$EXTRA"/*.txt "$EXTRA/hosts" "$ANTIGO/" 2>/dev/null || true
	rm -rf "$EXTRA"
fi

# Firefox do Ubuntu (snap) → Firefox do KalangOS: o perfil fica guardado.
if [ -d snap/firefox/common/.mozilla/firefox ]; then
	mkdir -p "$ANTIGO"
	echo ">> O perfil do Firefox (snap) ficou em ~/snap/firefox. O Firefox ESR do KalangOS não abre"
	echo "   perfis de versões mais novas: use o Firefox Sync, ou instale o Firefox da Loja (Flathub)."
fi

echo
echo "== Restaurado."
echo "   Listas do sistema antigo (pacotes, snaps, extensões): $ANTIGO"
echo "   Falta reinstalar o que vem da internet: rustup (Rust), ESP-IDF, SDK do Android,"
echo "   e fazer login de novo no gh (gh auth login), que guarda o acesso no chaveiro."
