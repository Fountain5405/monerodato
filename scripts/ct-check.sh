#!/usr/bin/env bash
# ct-check.sh — branch-structure check for the s64 field ops in a compiled binary/lib.
# Usage: ct-check.sh <binary-or-object> [<objdump>]   (objdump defaults to objdump)
# Reports instruction and conditional-jump counts per op_* wrapper, then per real body
# reached via GOT slots. Zero conditional jumps in a body = branch-free = CT candidate
# (loops with constant trip counts and cmov-based selects are the accepted exceptions;
# this script reports, a human classifies any nonzero result).
set -u
BIN=${1:?usage: ct-check.sh <binary> [objdump]}
OD=${2:-objdump}

ASM=$(mktemp)
"$OD" -d "$BIN" > "$ASM" 2>/dev/null || { echo "disassembly failed" >&2; exit 1; }

echo "== op wrappers (inline bodies) =="
for fn in op_add op_sub op_mul op_square op_to_repr op_ct_eq op_pow op_invert op_wide_reduce; do
  awk -v fn="$fn" '
    $0 ~ ("ct_inspect[0-9]+" fn ">:") { infn=1; next }
    infn == 1 && /^$/ { exit }
    infn == 1 && /:.*[[:space:]]/ { instr++ }
    infn == 1 && /\sj[a-z]+ / { j++; jd=jd "    " $0 "\n" }
    infn == 1 && /call[[:space:]]+\*/ { t=1 }
    END { printf "%-14s %4d instr %3d jumps%s\n", fn, instr+0, j+0, (t?"  (delegates via GOT)":""); printf "%s", jd }' "$ASM"
done

echo
echo "== delegated bodies (via R_X86_64_RELATIVE GOT slots) =="
DYN=$((0x581d0 - 0x240))  # derived from this binary's layout; adjust if it moves
slot() {
  readelf -r "$BIN" 2>/dev/null | awk -v s="$(printf '%012x' "$1")" 'tolower($1)==s {print $4}'
}
for name_slot in "op_to_repr:$((0x581d0))" "op_ct_eq:$((0x58210))" "op_pow:$((DYN+0x270))" \
  "op_wide_reduce:$((DYN+0x250))" "op_invert:$((DYN+0x288))"; do
  name=${name_slot%%:*}; s=${name_slot#*:}
  addr=$(slot "$s")
  [ -z "$addr" ] && { echo "$name: slot $(printf '%x' "$s") unresolved"; continue; }
  "$OD" -d --start-address="0x$addr" --stop-address=$((0x$addr + 0x2000)) "$BIN" 2>/dev/null |
    awk '/\t/ { instr++; if ($0 ~ /\sj[a-z]+ /) { j++; jd=jd "    " $0 "\n" } }
         instr > 0 && /^[[:space:]]*$/ { exit }
         END { printf "%-14s %4d instr %3d jumps\n%s", "'"$name"'", instr+0, j+0, substr(jd, 1, 800) }'
done
rm -f "$ASM"
