# ELYTerminal: load the usual startup files, report folder and title, mark prompts and commands.
# A Claude tab's id and prelude stay the shell's own, for its first prompt.
_ely_cl_ask=${ELY_CLAUDE-} _ely_cl_pre=${ELY_CLAUDE_PRELUDE-}
builtin unset ELY_CLAUDE ELY_CLAUDE_PRELUDE
if [ -n "${ELY_BASH_LOGIN-}" ]; then
  builtin unset ELY_BASH_LOGIN
  [ -r /etc/profile ] && builtin source /etc/profile
  for _ely_file in ~/.bash_profile ~/.bash_login ~/.profile; do
    if [ -r "$_ely_file" ]; then
      builtin source "$_ely_file"
      break
    fi
  done
  builtin unset _ely_file
else
  [ -r ~/.bashrc ] && builtin source ~/.bashrc
fi
_ely_ran=
_ely_n=0
_ely_ps0=
# MSYS's bash names a folder as Windows does: pwd -W.
_ely_msys=
builtin pwd -W >/dev/null 2>&1 && _ely_msys=1
if (( BASH_VERSINFO[0] * 100 + BASH_VERSINFO[1] >= 404 )); then
  # PS0 runs in the shell, so its subscript can note that a command ran.
  _ely_ps0='${_ely_note[_ely_ran=1]-}\e]133;C\a'
fi
_ely_prompt() {
  builtin local ret=$? LC_ALL=C char url= win= i title dir="$PWD"
  if [ -n "$_ely_ran" ]; then
    builtin printf '\e]133;D;%s\a' "$ret"
    _ely_ran=
  fi
  [ -n "$_ely_ps0" ] || _ely_d=([1]=$'\e]133;D;'"$ret"$'\a')
  if [ -n "${ELY_WSL-}" ]; then
    # Under WSL the folder goes as Windows names it: a drive's, or the distribution's share.
    dir=$(wslpath -w "$PWD" 2>/dev/null) || dir=
    dir=${dir//\\//}
    case $dir in //*) ;; ?:*) dir=/$dir ;; *) dir= ;; esac
  elif [ -n "$_ely_msys" ]; then
    # C:/x or //host/share; x keeps a name's last newline.
    dir=$(builtin pwd -W; builtin printf x)
    dir=${dir%$'\nx'}
    # Windows holds controls and "*:<>?| at U+F000 up.
    win=${dir:0:2}
    for (( i = 2; i < ${#dir}; i++ )); do
      char=${dir:i:1}
      if [[ $char == [$'\x01'-$'\x1f'] || '"*:<>?|' == *"$char"* ]]; then
        builtin printf -v char '%d' "'$char"
        builtin printf -v char '\\xef\\x%02x\\x%02x' $(( 0x80 + (char >> 6) )) $(( 0x80 + (char & 63) ))
        builtin printf -v char '%b' "$char"
      fi
      win+=$char
    done
    dir=$win
    case $dir in //*) ;; ?:*) dir=/$dir ;; *) dir= ;; esac
  fi
  for (( i = 0; i < ${#dir}; i++ )); do
    char=${dir:i:1}
    case $char in
      [[:cntrl:]]|%) builtin printf -v char '%%%02X' "'$char" ;;
    esac
    url+=$char
  done
  title=${PWD##*/}
  [ -n "$dir" ] && builtin printf '\e]7;file://%s\a' "$url"
  builtin printf '\e]2;%s\a' "${title//[[:cntrl:]]/}"
  builtin return $ret
}
# Claude as a marked command of its own, once, after every other prompt command; no eval of its id.
_ely_claude() {
  builtin local word= id=${_ely_cl_ask#*:} line= ret old errexit= re='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
  case ${_ely_cl_ask%%:*} in
    new) word=--session-id ;;
    resume) word=--resume ;;
  esac
  line="claude $word $id"
  [ -n "$_ely_cl_pre" ] && line=$_ely_cl_pre$'\n'$line
  _ely_cl_ask= _ely_cl_pre=
  # A bare D before the prompt tells the terminal Claude does not start.
  if [ -z "$word" ] || ! [[ $id =~ $re ]]; then
    builtin printf 'ELYTerminal: ELY_CLAUDE names no Claude session, so claude does not start\n' >&2
    builtin printf '\e]133;D\a'
    builtin return 0
  fi
  if ! [ -t 0 ] || ! [ -t 1 ]; then
    builtin printf 'ELYTerminal: claude needs the terminal, so it does not start\n' >&2
    builtin printf '\e]133;D\a'
    builtin return 0
  fi
  builtin history -s "$line"
  [[ $- == *e* ]] && errexit=1 && builtin set +e
  # Ctrl-C ends claude, not the prompt command, so its end is marked.
  old=$(builtin trap -p INT)
  builtin trap : INT
  builtin printf '\e]133;A;aid=elyterminal\a\e]133;C\a\e]2;claude\a'
  builtin eval "$line"
  ret=$?
  builtin printf '\e]133;D;%s\a' "$ret"
  builtin trap - INT
  [ -n "$old" ] && builtin eval "$old"
  [ -n "$errexit" ] && builtin set -e
  builtin return 0
}
# Last, after anything that sets PS1 or PS0.
_ely_mark() {
  if [[ $PS1 != *'\e]133;A'* ]]; then
    if [ -z "$_ely_ps0" ] && builtin shopt -q promptvars; then
      # bash 3.2 has no PS0; \#, the command number, moves once a command ran.
      PS1='\[${_ely_d[_ely_n && \# != _ely_n]-}${_ely_note[_ely_n = \#]-}\e]133;A;aid=elyterminal\a\]'"$PS1"'\[\e]133;B\a\]'
    else
      PS1='\[\e]133;A;aid=elyterminal\a\]'"$PS1"'\[\e]133;B\a\]'
    fi
  fi
  if [ -n "$_ely_ps0" ] && builtin shopt -q promptvars && [[ ${PS0-} != *"$_ely_ps0"* ]]; then
    PS0=$_ely_ps0${PS0-}
  fi
  if [ -n "$_ely_cl_ask" ]; then _ely_claude; fi
}
PROMPT_COMMAND="_ely_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND};_ely_mark"
# PS0 and the prompt mark a command only through expansions, which promptvars turns on.
builtin shopt -q promptvars || builtin printf 'ELYTerminal: bash marks no command while promptvars is off (shopt -s promptvars)\n' >&2
