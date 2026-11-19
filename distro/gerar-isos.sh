#!/bin/sh
# Gera as ISOs do KalangOS com o pacote kalango-shell já publicado no apt e,
# se pedido, envia para a pasta de downloads do site.
#
# Uso:
#   distro/gerar-isos.sh [padrao|hwe|todas] [--enviar]
#     padrao  edição padrão (kernel do Debian 13, máquinas antigas)
#     hwe     edição HWE (kernel, Mesa e firmware novos dos backports)
#     todas   as duas, uma depois da outra (padrão do script)
#     --enviar  depois de gerar, manda a ISO e o .sha256 para o servidor e
#               confere a soma lá
#
# Variáveis:
#   KALANGO_REPO         pasta do repositório apt (padrão ~/.local/share/kalango-repo;
#                        no desktop do Tiago: ~/Documents/.local/share/kalango-repo,
#                        achada sozinha se a padrão não existir)
#   KALANGO_ISO_DESTINO  destino do --enviar (padrão root@10.8.0.1:/www/wwwroot/kalangos.org/download)
#   KALANGO_SSH_CHAVE    chave SSH do envio (padrão ~/.ssh/id_tiago_desktop)
#
# O pacote é sempre o .deb mais novo do pool do apt: a ISO e as atualizações
# ficam com o mesmo arquivo (ver distro/repo/LEIAME.md). O build.sh pede sudo:
# a senha é pedida uma vez no começo e mantida viva até o fim, e todas as ISOs
# são geradas antes do envio (o envio de uma ISO leva horas numa conexão lenta;
# antes, o sudo expirava no meio e a segunda edição não era gerada).
# Não atualiza a página do site: o link, o tamanho e o SHA-256 novos ficam no
# resumo do fim, para pôr em kalangos-site/public/{,pt/}index.html.
set -eu

DISTRO=$(cd "$(dirname "$0")" && pwd)
OUT="$DISTRO/out"

EDICOES=todas
ENVIAR=0
for arg in "$@"; do
	case "$arg" in
	padrao | hwe | todas) EDICOES=$arg ;;
	--enviar) ENVIAR=1 ;;
	-h | --help)
		sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "Opção desconhecida: $arg (use padrao, hwe, todas ou --enviar)" >&2
		exit 1
		;;
	esac
done
[ "$EDICOES" = todas ] && EDICOES="padrao hwe"

# Repositório apt: o indicado, o padrão ou o do desktop do Tiago.
REPO=${KALANGO_REPO:-$HOME/.local/share/kalango-repo}
[ -d "$REPO/www/apt/pool" ] || REPO="$HOME/Documents/.local/share/kalango-repo"
POOL="$REPO/www/apt/pool/main/k/kalango-shell"
[ -d "$POOL" ] || { echo "Não achei o pool do apt em $POOL (defina KALANGO_REPO)." >&2; exit 1; }

# O .deb de versão mais alta (comparação do dpkg: 0.1.1-10 > 0.1.1-9).
DEB=""
for f in "$POOL"/kalango-shell_*_amd64.deb; do
	[ -f "$f" ] || continue
	if [ -z "$DEB" ] || dpkg --compare-versions "$(dpkg-deb -f "$f" Version)" gt "$(dpkg-deb -f "$DEB" Version)"; then
		DEB=$f
	fi
done
[ -n "$DEB" ] || { echo "Nenhum kalango-shell .deb em $POOL." >&2; exit 1; }
echo ">> Pacote: $(basename "$DEB") (o mais novo publicado no apt)"

DESTINO=${KALANGO_ISO_DESTINO:-root@10.8.0.1:/www/wwwroot/kalangos.org/download}
CHAVE=${KALANGO_SSH_CHAVE:-$HOME/.ssh/id_tiago_desktop}
SSH="ssh -i $CHAVE -o BatchMode=yes"

# Senha do sudo agora, enquanto a pessoa está olhando, e mantida viva.
sudo -v
( while kill -0 $$ 2>/dev/null; do sudo -n true 2>/dev/null; sleep 50; done ) &
MANTER_SUDO=$!
trap 'kill $MANTER_SUDO 2>/dev/null' EXIT INT TERM

RESUMO=""
GERADAS=""
for ed in $EDICOES; do
	echo
	echo ">> Gerando a edição $ed ($(date +%H:%M))"
	antes=$(date +%s)
	KALANGO_EDICAO=$ed KALANGO_DEB=$DEB "$DISTRO/build.sh"
	# A ISO que o build.sh acabou de gerar (a mais nova desta edição).
	if [ "$ed" = hwe ]; then
		iso=$(ls -t "$OUT"/kalango-os-*-hwe.iso | head -n 1)
	else
		iso=$(ls -t "$OUT"/kalango-os-*.iso | grep -v -- '-hwe\.iso$' | head -n 1)
	fi
	[ "$(stat -c %Y "$iso")" -ge "$antes" ] || { echo "A ISO $iso não foi gerada agora; algo falhou." >&2; exit 1; }
	nome=$(basename "$iso")
	(cd "$OUT" && sha256sum "$nome" >"$nome.sha256")
	soma=$(cut -d' ' -f1 "$OUT/$nome.sha256")
	tamanho=$(du -h --apparent-size "$iso" | cut -f1)
	echo ">> Pronta: $nome ($tamanho)"
	GERADAS="$GERADAS $iso"
	RESUMO="$RESUMO
  $ed: $nome ($tamanho)
       SHA-256 $soma"
done

# Envio só depois de todas geradas.
if [ "$ENVIAR" = 1 ]; then
	host=${DESTINO%%:*}
	pasta=${DESTINO#*:}
	for iso in $GERADAS; do
		nome=$(basename "$iso")
		echo
		echo ">> Enviando $nome para $DESTINO ($(date +%H:%M))"
		rsync -t --partial --progress --chmod=F644 -e "$SSH" "$iso" "$iso.sha256" "$DESTINO/"
		$SSH "$host" "cd '$pasta' && chown www:www '$nome' '$nome.sha256' && sha256sum -c '$nome.sha256'"
	done
fi

echo
echo ">> Tudo pronto. Pacote $(basename "$DEB").$RESUMO"
[ "$ENVIAR" = 1 ] && echo "   Enviadas para $DESTINO (soma conferida no servidor)."
echo "   Falta pôr o link e o SHA-256 na página do site (kalangos-site/public/index.html e pt/index.html)."
