# Cor da seleção do terminal na cor de destaque do KalangOS (como no
# terminal novo do Ubuntu). Lido pelo ~/.zshrc do KalangOS.
#
# O Ptyxis do Debian 13 não tem opção para a cor de seleção; o VTE (o motor
# dele) aceita a cor pelo código OSC 17 (fundo da seleção) e OSC 19 (texto),
# que o shell manda ao abrir cada aba. Só em terminais VTE (Ptyxis, GNOME
# Terminal): os outros ignorariam ou mostrariam lixo.
if [ -n "${VTE_VERSION:-}" ] && [ -t 1 ]; then
	_kalango_destaque=$(sed -n 's/^destaque=//p' "${XDG_CONFIG_HOME:-$HOME/.config}/kalango/aparencia.conf" 2>/dev/null | head -n 1)
	case "$_kalango_destaque" in
	1) _kalango_cor='#a78bfa' ;;
	2) _kalango_cor='#7ea6f7' ;;
	3) _kalango_cor='#5fd38d' ;;
	4) _kalango_cor='#e8b04b' ;;
	5) _kalango_cor='#f28b95' ;;
	*) _kalango_cor='#3dd6f5' ;;
	esac
	printf '\033]17;%s\007\033]19;#07090d\007' "$_kalango_cor"
	unset _kalango_destaque _kalango_cor
fi
