#!/bin/sh
# ELYTerminal's statusLine wrapper: keeps the input that holds Claude's limits, then runs the original line.
umask 077
dir=${0%/*}
input=$(cat; printf x)
input=${input%x}
case $input in
*'"rate_limits"'*)
  printf '%s' "$input" >"$dir/claude-rate.json.$$" &&
    mv -f "$dir/claude-rate.json.$$" "$dir/claude-rate.json"
  ;;
esac
inner=$(cat "$dir/claude-inner" 2>/dev/null)
[ -n "$inner" ] || exit 0
exec /bin/sh -c "$inner" <<ELY_INPUT
$input
ELY_INPUT
