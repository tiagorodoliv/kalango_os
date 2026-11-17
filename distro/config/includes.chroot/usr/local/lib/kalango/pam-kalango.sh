#!/bin/sh
# KalangOS: ajustes do PAM (senhas e PIN), como no Ubuntu e no Windows Hello.
#
# 1. Qualquer senha, até de um caractere. Duas barreiras do Debian impediam:
#    - o pam_unix, ao trocar a própria senha (Configurações → Usuários,
#      passwd): `obscure` recusa senha "simples" e o `minlen` padrão é 6;
#    - o Calamares, na instalação, pelo libpwquality (mínimo de 6).
#
# 2. PIN (opcional, por pessoa) na tela de login e no desbloqueio: uma linha
#    pam_exec com o /usr/local/sbin/kalango-pin entra ANTES do `login` nos
#    serviços greetd, gtklock e swaylock. Se não for o PIN, o pam_unix confere
#    a mesma digitação como senha (`try_first_pass`), sem perguntar de novo.
#    sudo, su e pkexec usam o common-auth direto: continuam pedindo a senha.
#
# O perfil /usr/share/pam-configs/unix (do libpam-runtime) é desviado com
# dpkg-divert para /usr/share/kalango/pam-unix.debian (FORA da pasta
# pam-configs: o pam-auth-update lê todo arquivo de lá, e a cópia virava um
# segundo perfil "unix.debian", com o pam_unix duas vezes). A versão do
# KalangOS é gerada a partir dela e o pam-auth-update monta os common-*. Uma
# atualização do libpam-runtime grava no arquivo desviado e a nossa continua.
#
# Uso: pam-kalango.sh ligar     (hook 0170 da ISO e postinst do kalango-shell)
#      pam-kalango.sh desligar  (prerm do kalango-shell: volta ao do Debian)
# Pode rodar quantas vezes quiser.
set -e

PERFIL=/usr/share/pam-configs/unix
ORIGINAL=/usr/share/kalango/pam-unix.debian
USERS=/etc/calamares/modules/users.conf
PIN=/usr/local/sbin/kalango-pin
PIN_DIR=/var/lib/kalango/pin
# Serviços com PIN: tela de login (greetd) e telas de bloqueio.
SERVICOS="greetd gtklock swaylock"

# Pacote dono do desvio: o kalango-shell (a ISO também o instala).
PACOTE=kalango-shell

gerar_perfil() {
	# As linhas do pam_unix começam com tab; os cabeçalhos de seção, não.
	#  - Password e Password-Initial: tira obscure e põe minlen=1;
	#  - Auth-Initial: try_first_pass, para aproveitar o que a linha do PIN
	#    já leu (no Debian só a variante "Auth" o tem).
	awk '
		/^[^\t]/ {
			senha = ($0 ~ /^Password(-Initial)?:/)
			auth0 = ($0 ~ /^Auth-Initial:/)
		}
		senha && /^\t.*pam_unix\.so/ {
			sub(/ obscure/, "")
			if ($0 !~ /minlen=/) sub(/pam_unix\.so/, "pam_unix.so minlen=1")
		}
		auth0 && /^\t.*pam_unix\.so/ && $0 !~ /try_first_pass/ {
			sub(/pam_unix\.so/, "pam_unix.so try_first_pass")
		}
		{ print }
	' "$ORIGINAL" >"$PERFIL.novo"
	mv "$PERFIL.novo" "$PERFIL"
}

# Põe as linhas do PIN num arquivo do /etc/pam.d (antes e depois do `login`).
pin_no_servico() {
	arq=/etc/pam.d/$1
	[ -f "$arq" ] || return 0
	grep -q kalango-pin "$arq" && return 0
	awk -v pin="$PIN" '
		!feito && /^(@include[ \t]+login|auth[ \t]+include[ \t]+login)/ {
			print "# KalangOS (PIN): login/desbloqueio com PIN (pam-kalango.sh); se não for"
			print "# KalangOS (PIN): o PIN, o pam_unix confere a mesma digitação como senha."
			print "auth [success=done default=ignore] pam_exec.so quiet expose_authtok " pin " verificar"
			print
			print "# KalangOS (PIN): entrou com a senha → zera as tentativas erradas de PIN."
			print "auth optional pam_exec.so quiet " pin " senha-ok"
			feito = 1
			next
		}
		{ print }
	' "$arq" >"$arq.kalango"
	if grep -q kalango-pin "$arq.kalango"; then
		cat "$arq.kalango" >"$arq"
	fi
	rm -f "$arq.kalango"
}

case "${1:-ligar}" in
ligar)
	if [ -f "$PERFIL" ] || [ -f "$ORIGINAL" ]; then
		mkdir -p "${ORIGINAL%/*}"
		if ! dpkg-divert --list "$PERFIL" | grep -q "$ORIGINAL"; then
			dpkg-divert --package "$PACOTE" --rename --divert "$ORIGINAL" --add "$PERFIL" >/dev/null
		fi
		gerar_perfil
		DEBIAN_FRONTEND=noninteractive pam-auth-update --package
	fi
	# PINs: cada pessoa cria o próprio arquivo; ninguém lista a pasta (1733,
	# como o /tmp sem leitura) e o sticky impede apagar o dos outros.
	install -d -m 0755 /var/lib/kalango
	install -d -m 1733 "$PIN_DIR"
	for s in $SERVICOS; do
		pin_no_servico "$s"
	done
	# Calamares (só existe na ISO): senha fraca vira aviso, com a caixa
	# "Permitir senhas fracas" já marcada.
	if [ -f "$USERS" ]; then
		for chave in allowWeakPasswords allowWeakPasswordsDefault; do
			if grep -q "^$chave:" "$USERS"; then
				sed -i "s/^$chave:.*/$chave: true/" "$USERS"
			else
				echo "$chave: true" >>"$USERS"
			fi
		done
	fi
	;;
desligar)
	for s in $SERVICOS; do
		arq=/etc/pam.d/$s
		[ -f "$arq" ] && sed -i '/KalangOS (PIN)\|kalango-pin/d' "$arq"
	done
	if dpkg-divert --list "$PERFIL" | grep -q "$ORIGINAL"; then
		rm -f "$PERFIL"
		dpkg-divert --package "$PACOTE" --rename --remove "$PERFIL" >/dev/null
		DEBIAN_FRONTEND=noninteractive pam-auth-update --package
	fi
	;;
*)
	echo "uso: $0 ligar|desligar" >&2
	exit 2
	;;
esac
