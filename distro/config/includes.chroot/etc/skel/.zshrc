# ~/.zshrc do KalangOS: prompt de duas linhas no estilo do Kali Linux, com
# as cores do Kalango (ciano e azul; root em vermelho).
#
#   ┌──(usuário㉿computador)-[~/pasta]
#   └─$
#
# Ctrl+P alterna entre duas linhas e uma linha. As cores do terminal vêm do
# tema escolhido em Configurações → Aparência → Terminal.

setopt autocd              # digitar só o nome da pasta entra nela
setopt interactivecomments # permite # comentários na linha de comando
setopt magicequalsubst     # completa nomes depois de "=" (ex.: --arq=~/...)
setopt nonomatch           # padrão sem resultado vai como texto, sem erro
setopt notify              # avisa na hora quando um processo em segundo plano acaba
setopt numericglobsort     # ordena arq2 antes de arq10
setopt promptsubst         # o prompt pode usar variáveis e comandos

WORDCHARS=${WORDCHARS//\/}  # Ctrl+W apaga até a barra, não o caminho todo
PROMPT_EOL_MARK=""          # sem o "%" quando a saída não termina em nova linha

# --- Teclas (modo emacs, como o bash) ---
bindkey -e
bindkey ' ' magic-space                         # expande !! e afins no espaço
bindkey '^U' backward-kill-line                 # Ctrl+U
bindkey '^[[3;5~' kill-word                     # Ctrl+Delete
bindkey '^[[3~' delete-char                     # Delete
bindkey '^[[1;5C' forward-word                  # Ctrl+→
bindkey '^[[1;5D' backward-word                 # Ctrl+←
bindkey '^[[5~' beginning-of-buffer-or-history  # Page Up
bindkey '^[[6~' end-of-buffer-or-history        # Page Down
bindkey '^[[H' beginning-of-line                # Home
bindkey '^[[F' end-of-line                      # End

# --- Completar com menu (Tab duas vezes navega pelas opções) ---
autoload -Uz compinit
mkdir -p ~/.cache
compinit -d ~/.cache/zcompdump
zstyle ':completion:*:*:*:*:*' menu select
zstyle ':completion:*' group-name ''
zstyle ':completion:*' format '%F{cyan}── %d ──%f'
zstyle ':completion:*' list-colors "${(s.:.)LS_COLORS}"
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}'
zstyle ':completion:*' rehash true
zstyle ':completion:*' verbose true

# --- Histórico ---
HISTFILE=~/.zsh_history
HISTSIZE=10000
SAVEHIST=20000
setopt hist_expire_dups_first hist_ignore_dups hist_ignore_space hist_verify share_history
alias history="history 0"

# --- Prompt ---
PROMPT_ALTERNATIVE=twoline
NEWLINE_BEFORE_PROMPT=yes

configure_prompt() {
    local simbolo='㉿'
    case "$PROMPT_ALTERNATIVE" in
        twoline)
            PROMPT=$'%F{%(#.red.cyan)}┌──${VIRTUAL_ENV:+($(basename $VIRTUAL_ENV))─}(%B%F{%(#.red.blue)}%n'$simbolo$'%m%b%F{%(#.red.cyan)})-[%B%F{reset}%(6~.%-1~/…/%4~.%5~)%b%F{%(#.red.cyan)}]\n└─%B%(#.%F{red}#.%F{blue}$)%b%F{reset} '
            ;;
        oneline)
            PROMPT=$'${VIRTUAL_ENV:+($(basename $VIRTUAL_ENV)) }%B%F{%(#.red.blue)}%n@%m%b%F{reset}:%B%F{%(#.red.cyan)}%~%b%F{reset}%(#.#.$) '
            ;;
    esac
    # À direita: código de saída do último comando (se falhou) e tarefas em
    # segundo plano.
    RPROMPT=$'%(?.. %? %F{red}%B✗%b%F{reset})%(1j. %j %F{yellow}%B⚙%b%F{reset}.)'
}

toggle_oneline_prompt() {
    if [ "$PROMPT_ALTERNATIVE" = oneline ]; then
        PROMPT_ALTERNATIVE=twoline
    else
        PROMPT_ALTERNATIVE=oneline
    fi
    configure_prompt
    zle reset-prompt
}
zle -N toggle_oneline_prompt
bindkey ^P toggle_oneline_prompt

configure_prompt

# Linha em branco entre um comando e o próximo prompt (não antes do primeiro)
# e título da janela "usuário@computador: pasta".
precmd() {
    print -Pnr -- $'\e]0;%n@%m: %~\a'
    if [ "$NEWLINE_BEFORE_PROMPT" = yes ]; then
        if [ -z "$_NEW_LINE_BEFORE_PROMPT" ]; then
            _NEW_LINE_BEFORE_PROMPT=1
        elif [ "$PROMPT_ALTERNATIVE" = twoline ]; then
            print ""
        fi
    fi
}

# --- Cores nos comandos ---
if [ -x /usr/bin/dircolors ]; then
    eval "$(dircolors -b)"
    alias ls='ls --color=auto'
    alias grep='grep --color=auto'
    alias diff='diff --color=auto'
    alias ip='ip --color=auto'
fi
alias ll='ls -l'
alias la='ls -A'
alias l='ls -CF'

# --- Sugestões (cinza, → aceita) e destaque de sintaxe ---
if [ -f /usr/share/zsh-autosuggestions/zsh-autosuggestions.zsh ]; then
    . /usr/share/zsh-autosuggestions/zsh-autosuggestions.zsh
    ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE='fg=8'
fi
if [ -f /usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh ]; then
    . /usr/share/zsh-syntax-highlighting/zsh-syntax-highlighting.zsh
    ZSH_HIGHLIGHT_STYLES[path]=underline
    ZSH_HIGHLIGHT_STYLES[command]=fg=green,bold
    ZSH_HIGHLIGHT_STYLES[builtin]=fg=green,bold
    ZSH_HIGHLIGHT_STYLES[alias]=fg=green,bold
    ZSH_HIGHLIGHT_STYLES[unknown-token]=fg=red,bold
fi

# "comando não encontrado" sugere o pacote (se o command-not-found estiver instalado).
if [ -f /etc/zsh_command_not_found ]; then
    . /etc/zsh_command_not_found
fi

# --- Ficha do sistema ao abrir o terminal (kalango-ficha: ícone + fastfetch) ---
# Só na janela nova do terminal (não em shells dentro de shells nem pela SSH).
# Para não aparecer mais: touch ~/.config/kalango/sem-fastfetch
if [[ -o interactive && -z $KALANGO_FASTFETCH && -z $SSH_CONNECTION ]] \
    && [[ ! -e ~/.config/kalango/sem-fastfetch ]] && (( $+commands[kalango-ficha] )); then
    export KALANGO_FASTFETCH=1
    kalango-ficha
fi
