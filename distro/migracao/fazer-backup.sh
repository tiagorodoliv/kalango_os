#!/bin/sh
# Backup da pasta pessoal e das conexões de rede para levar a outro sistema
# (ex.: do Ubuntu para o KalangOS, antes de formatar). Restaura com
# restaurar-backup.sh.
#
# Uso:
#   ./fazer-backup.sh DESTINO              cria DESTINO/backup-<usuário>-<data>.tar.zst
#   ./fazer-backup.sh --senha DESTINO      o mesmo, criptografado (gpg, AES256)
#   ./fazer-backup.sh --simular            só mostra o que entra e o tamanho
#   --tudo                                 inclui também ESP-IDF e SDK do Android
#
# DESTINO precisa estar em OUTRO disco (pendrive/HD externo/rede): o disco
# deste computador será formatado.
#
# Entra: a pasta pessoal inteira (arquivos, projetos com o histórico do git,
# chaves SSH/GPG, chaveiro de senhas, Claude, navegadores, VS Code...), as
# conexões salvas do NetworkManager (Wi-Fi e VPN) e listas dos programas
# instalados. Fica de fora o que se baixa ou recompila de novo (caches,
# target/ do Rust, node_modules, toolchains).
set -eu

SENHA=0; SIMULAR=0; TUDO=0; DESTINO=
for a in "$@"; do
	case "$a" in
		--senha) SENHA=1 ;;
		--simular) SIMULAR=1 ;;
		--tudo) TUDO=1 ;;
		-h|--help) sed -n '2,20p' "$0"; exit 0 ;;
		*) DESTINO=$a ;;
	esac
done
[ "$(id -u)" -ne 0 ] || { echo "Rode como o seu usuário (o sudo é pedido só para a parte de rede)."; exit 1; }
[ "$SIMULAR" = 1 ] || [ -n "$DESTINO" ] || { echo "Uso: $0 [--senha] [--tudo] DESTINO   (ou --simular)"; exit 1; }

# --- O que não entra (relativo à pasta pessoal) ---
EXCLUIR=$(mktemp)
cat >"$EXCLUIR" <<'LISTA'
./.cache
./.local/share/Trash
./.gradle/caches
./.gradle/wrapper/dists
./.npm
./.pub-cache
./.dartServer
./.rustup
./.cargo/registry
./.cargo/git
./.vscode/extensions
./snap/*/common/.cache
./.config/google-chrome/*/Cache
./.config/google-chrome/*/Code Cache
./.config/google-chrome/*/GPUCache
./.config/google-chrome/*/Service Worker/CacheStorage
./.config/BraveSoftware/*/*/Cache
./.config/BraveSoftware/*/*/Code Cache
./.config/Code/Cache*
./.config/Code/CachedData
./.config/Code/logs
./.config/Claude/Cache
./.config/Claude/Code Cache
*/target
*/node_modules
*/.gradle
*/build/intermediates
*/.dart_tool
./git/kalango_os/distro/.cache
./git/kalango_os/distro/out
LISTA
if [ "$TUDO" = 0 ]; then
	printf '%s\n' ./.espressif ./esp ./Android ./.android/avd ./.android/cache >>"$EXCLUIR"
fi

cd "$HOME"
if [ "$SIMULAR" = 1 ]; then
	echo ">> Calculando o que entra no backup (pode levar alguns minutos)..."
	tar --create --file=/dev/null --exclude-from="$EXCLUIR" --exclude-caches-all \
		--one-file-system --totals . 2>&1 | grep -v 'socket ignored' | tail -2
	echo ">> Maiores itens que entram (já sem o que fica de fora):"
	for item in .[!.]* *; do
		[ -e "$item" ] || continue
		b=$(tar --create --file=/dev/null --exclude-from="$EXCLUIR" --exclude-caches-all \
			--one-file-system --totals "./$item" 2>&1 | sed -n 's/^Total bytes written: \([0-9]*\).*/\1/p')
		[ "${b:-0}" -gt 104857600 ] && printf '%s\t%s\n' "$b" "$item"
	done | sort -rn | head -15 | awk -F'\t' '{printf "  %6.1f GB  %s\n", $1/1073741824, $2}'
	rm -f "$EXCLUIR"
	exit 0
fi

# O destino não pode estar no disco que será formatado.
mkdir -p "$DESTINO"
DESTINO=$(cd "$DESTINO" && pwd)
if [ "$(stat -c %d "$DESTINO")" = "$(stat -c %d "$HOME")" ]; then
	echo "!! $DESTINO está no mesmo disco da pasta pessoal. Use um pendrive, HD externo ou pasta de rede."
	exit 1
fi

NOME="backup-$(id -un)-$(date +%Y-%m-%d_%H%M)"
EXTRA="$HOME/.kalango-backup-extra"
rm -rf "$EXTRA"; mkdir -p "$EXTRA/rede"

echo ">> Listas dos programas instalados (só referência, para reinstalar)"
dpkg-query -W -f='${Package}\n' >"$EXTRA/pacotes-apt.txt" 2>/dev/null || true
apt-mark showmanual >"$EXTRA/pacotes-apt-manuais.txt" 2>/dev/null || true
snap list 2>/dev/null | awk 'NR>1{print $1}' >"$EXTRA/snaps.txt" || true
flatpak list --app --columns=application >"$EXTRA/flatpaks.txt" 2>/dev/null || true
code --list-extensions >"$EXTRA/vscode-extensoes.txt" 2>/dev/null || true
crontab -l >"$EXTRA/crontab.txt" 2>/dev/null || true

echo ">> Conexões de rede salvas (Wi-Fi, VPN): pede a senha do sudo"
if [ -d /etc/NetworkManager/system-connections ]; then
	sudo sh -c "cp -a /etc/NetworkManager/system-connections/. '$EXTRA/rede/' && chown -R $(id -u):$(id -g) '$EXTRA/rede'"
fi
[ -d /etc/wireguard ] && sudo sh -c "mkdir -p '$EXTRA/wireguard' && cp -a /etc/wireguard/. '$EXTRA/wireguard/' && chown -R $(id -u):$(id -g) '$EXTRA/wireguard'" || true
cp /etc/hosts "$EXTRA/hosts" 2>/dev/null || true

ARQ="$DESTINO/$NOME.tar.zst"
[ "$SENHA" = 1 ] && ARQ="$ARQ.gpg"
echo ">> Compactando a pasta pessoal em $ARQ"
echo "   (dezenas de GB levam de 10 a 40 min; não desligue nem remova o disco)"
compactar() {
	tar --create --exclude-from="$EXCLUIR" --exclude-caches-all --one-file-system \
		--warning=no-file-ignored --warning=no-file-changed . 2>"$DESTINO/$NOME.erros.txt" |
		zstd -T0 -6 -q
}
if [ "$SENHA" = 1 ]; then
	echo "   Escolha uma senha forte e GUARDE-A: sem ela não há como restaurar."
	compactar | gpg --symmetric --cipher-algo AES256 --pinentry-mode loopback -o "$ARQ"
else
	compactar >"$ARQ"
fi
rm -rf "$EXTRA" "$EXCLUIR"

echo ">> Conferindo o backup (lendo o arquivo de volta)"
listar() {
	if [ "$SENHA" = 1 ]; then gpg -d --pinentry-mode loopback -q "$ARQ"; else cat "$ARQ"; fi | zstd -dq | tar -t
}
listar >"$DESTINO/$NOME.conteudo.txt"
faltou=0
for item in ./.ssh/ ./.gitconfig ./.claude/ ./.kalango-backup-extra/rede/; do
	[ -e "$HOME/${item#./}" ] || [ "$item" = ./.kalango-backup-extra/rede/ ] || continue
	grep -q "^${item}" "$DESTINO/$NOME.conteudo.txt" || { echo "!! FALTOU no backup: $item"; faltou=1; }
done
[ -d "$HOME/.local/share/kalango-repo" ] && { grep -q '^./.local/share/kalango-repo/' "$DESTINO/$NOME.conteudo.txt" || { echo "!! FALTOU a chave do repositório apt"; faltou=1; }; }
sha256sum "$ARQ" >"$ARQ.sha256"
echo
echo "== Pronto: $ARQ ($(du -h "$ARQ" | cut -f1), $(wc -l <"$DESTINO/$NOME.conteudo.txt") arquivos)"
[ -s "$DESTINO/$NOME.erros.txt" ] && echo "   Avisos do tar em $DESTINO/$NOME.erros.txt (arquivos sem permissão de leitura)."
[ "$faltou" = 0 ] && echo "   Conferido: chaves SSH, git, Claude e conexões de rede estão no backup." || exit 1
