# ELYTerminal: restore ZDOTDIR, load the user's .zshenv, report folder and title, mark prompts and commands.
# A Claude tab's id and prelude stay the shell's own, for its first prompt.
_ely_cl_ask=${ELY_CLAUDE-} _ely_cl_pre=${ELY_CLAUDE_PRELUDE-}
builtin unset ELY_CLAUDE ELY_CLAUDE_PRELUDE
if [[ -n "${ELY_ZSH_ZDOTDIR+set}" ]]; then
  ZDOTDIR=$ELY_ZSH_ZDOTDIR
  builtin unset ELY_ZSH_ZDOTDIR
else
  builtin unset ZDOTDIR
fi
[[ -r "${ZDOTDIR:-$HOME}/.zshenv" ]] && builtin source "${ZDOTDIR:-$HOME}/.zshenv"
if [[ -o interactive ]]; then
  _ely_ran=
  _ely_first=1
  _ely_precmd() {
    builtin local ret=$? LC_ALL=C char url= dir=$PWD
    # The user's options may not bend the script.
    builtin setopt local_options unset no_ksh_arrays no_sh_word_split no_err_return no_err_exit
    if [[ -n $_ely_ran ]]; then
      builtin printf '\e]133;D;%s\a' $ret
      _ely_ran=
    fi
    _ely_cl_now=
    if [[ -n ${ELY_WSL-} ]]; then
      # Under WSL the folder goes as Windows names it: a drive's, or the distribution's share.
      dir=${"$(wslpath -w $PWD 2>/dev/null)"//\\//}
      case $dir in //*) ;; ?:*) dir=/$dir ;; *) dir= ;; esac
    fi
    for char in ${(s::)dir}; do
      case $char in
        [[:cntrl:]]|%) builtin printf -v char '%%%02X' "'$char" ;;
      esac
      url+=$char
    done
    [[ -n $dir ]] && builtin printf '\e]7;file://%s\a' "$url"
    builtin printf '\e]2;%s\a' "${${${PWD:t}:-/}//[[:cntrl:]]/}"
    # A theme holding the last hook rewrites PS1: mark before it.
    # A widget running the hooks is mid-line: no mark there.
    if (( ! _ely_first )) && ! builtin zle && [[ ${precmd_functions[-1]} != _ely_mark ]]; then
      builtin local redraw=
      [[ -o zle ]] && redraw=';redraw=1'
      builtin printf '\e]133;A;aid=elyterminal%s\a' $redraw
    fi
    # Themes set PS1 in later hooks: the marks go on after them, at the first prompt through sched.
    precmd_functions=(${precmd_functions:#_ely_mark} _ely_mark)
    if (( _ely_first )); then
      _ely_first=0
      [[ -n $_ely_cl_ask ]] && _ely_claude
      [[ -o prompt_percent ]] || builtin print -ru2 'ELYTerminal: zsh marks no prompt while prompt_percent is off (setopt prompt_percent)'
      builtin zmodload zsh/sched && builtin sched +0 _ely_mark $ret
    fi
    builtin return ret
  }
  _ely_mark() {
    builtin setopt local_options unset no_ksh_arrays no_sh_word_split no_err_return no_err_exit
    builtin local a=$'\e]133;A;aid=elyterminal' p=$'\e]133;P;k=s' r=';redraw=1'
    if [[ -o prompt_percent && $PS1 != *$'\e]133;A'* ]]; then
      PS1=$'%{'$a$'\a%}'$PS1$'%{\e]133;B\a%}'
    fi
    # Only the line editor redraws a prompt on a resize, and it may go.
    PS1=${PS1//"$a$r"/$a}
    [[ -o zle ]] && PS1=${PS1//"$a"/$a$r}
    # An unset PS2 stays unset.
    if (( ${+PS2} )); then
      if [[ -o prompt_percent && $PS2 != *$'\e]133;P'* ]]; then
        PS2=$'%{'$p$'\a%}'$PS2$'%{\e]133;B\a%}'
      fi
      PS2=${PS2//"$p$r"/$p}
      [[ -o zle ]] && PS2=${PS2//"$p"/$p$r}
    fi
    # sched's command leaves its status for the prompt.
    builtin return ${1:-0}
  }
  # Claude's line, typed by the line editor at the first prompt; no eval of its id.
  _ely_claude() {
    builtin setopt local_options unset no_ksh_arrays no_sh_word_split no_err_return no_err_exit
    builtin local word= id=${_ely_cl_ask#*:} re='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
    case ${_ely_cl_ask%%:*} in
      new) word=--session-id ;;
      resume) word=--resume ;;
    esac
    _ely_cl_ask=
    # A bare D before the prompt tells the terminal Claude does not start.
    if [[ -z $word || ! $id =~ $re ]]; then
      builtin print -ru2 'ELYTerminal: ELY_CLAUDE names no Claude session, so claude does not start'
      builtin printf '\e]133;D;aid=elyterminal\a'
    elif [[ ! -o zle ]]; then
      builtin print -ru2 'ELYTerminal: claude does not start while zle is off'
      builtin printf '\e]133;D;aid=elyterminal\a'
    else
      _ely_cl_line="claude $word $id"
      [[ -n $_ely_cl_pre ]] && _ely_cl_line=$_ely_cl_pre$'\n'$_ely_cl_line
      builtin autoload -Uz add-zle-hook-widget
      add-zle-hook-widget zle-line-init _ely_claude_line
    fi
    _ely_cl_pre=
  }
  # Once; add-zle-hook-widget runs it after the hooks before it, and a failing one before it stops it, as zsh runs them.
  _ely_claude_line() {
    add-zle-hook-widget -d zle-line-init _ely_claude_line
    [[ -n $_ely_cl_line ]] || builtin return 0
    BUFFER=$_ely_cl_line
    _ely_cl_line=
    _ely_cl_now=1
    builtin zle .accept-line
  }
  _ely_preexec() {
    _ely_ran=1
    # Claude's command starts right after a prompt start of its own, which tells it from a line typed ahead.
    if [[ -n ${_ely_cl_now-} ]]; then
      _ely_cl_now=
      builtin printf '\e]133;A;aid=elyterminal\a'
    fi
    builtin printf '\e]133;C\a\e]2;%s\a' "${1//[[:cntrl:]]/}"
  }
  builtin autoload -Uz add-zsh-hook
  add-zsh-hook precmd _ely_precmd
  add-zsh-hook precmd _ely_mark
  add-zsh-hook preexec _ely_preexec
fi
