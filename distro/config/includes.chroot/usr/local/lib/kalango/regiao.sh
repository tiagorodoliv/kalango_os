# Idioma e teclado da sessão do KalangOS (lido com "." pelo kalango-session,
# kalango-greeter e kalango-instalar).
#
# Ordem (o último vence):
#   1. /etc/default/locale e /etc/default/keyboard: escritos pelo live-config
#      na ISO (parâmetros locales= e keyboard-layouts= do boot) e pelo
#      Calamares no sistema instalado;
#   2. ~/.config/kalango/regiao.conf: a escolha do usuário em
#      Configurações → Idioma e região (LANG, LANGUAGE, XKBLAYOUT, XKBVARIANT).
# Sem nada disso: inglês (en_US) e teclado us.
#
# O greetd não roda um shell de login, então nada disso seria lido sozinho.
# O labwc usa as variáveis XKB_DEFAULT_* para o teclado.
kalango_regiao() {
	if [ -r /etc/default/locale ]; then
		. /etc/default/locale
	fi
	if [ -r /etc/default/keyboard ]; then
		. /etc/default/keyboard
	fi
	_conf="${XDG_CONFIG_HOME:-$HOME/.config}/kalango/regiao.conf"
	if [ -n "${HOME:-}" ] && [ -r "$_conf" ]; then
		. "$_conf"
	fi
	export LANG="${LANG:-en_US.UTF-8}"
	if [ -n "${LANGUAGE:-}" ]; then
		export LANGUAGE
	else
		unset LANGUAGE
	fi
	export XKB_DEFAULT_LAYOUT="${XKBLAYOUT:-us}"
	if [ -n "${XKBVARIANT:-}" ]; then
		export XKB_DEFAULT_VARIANT="$XKBVARIANT"
	fi
	if [ -n "${XKBMODEL:-}" ]; then
		export XKB_DEFAULT_MODEL="$XKBMODEL"
	fi
	if [ -n "${XKBOPTIONS:-}" ]; then
		export XKB_DEFAULT_OPTIONS="$XKBOPTIONS"
	fi
}
