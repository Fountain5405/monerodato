#!/bin/sh
# monerodato (fork-only): check, before every build of fcmp_pp_rust, that the trees in
# src/fcmp_pp/fcmp_pp_rust/patched/ and the vendored crates sit on the same upstream
# revision. It catches an update that moves one side only (for example a new monero-oxide
# rev in Cargo.toml without re-importing patched/monero-oxide, or the reverse).
#
# Checks:
#  1. every monero-oxide git package in Cargo.lock is pinned to the revision that
#     patched/bases.txt gives for patched/monero-oxide;
#  2. every vendored monero-oxide crate is, file for file, the same as its copy in
#     patched/monero-oxide (cargo's generated Cargo.toml and checksum files excepted);
#  3. dalek-ff-group in Cargo.lock is the patched path package, at the version
#     patched/bases.txt gives;
#  4. every multiexp package in Cargo.lock is the patched path package, at the version
#     patched/bases.txt gives, and vendor/ holds no multiexp. A second, registry multiexp
#     (for example one a monero-oxide update pulls in) would build without the patch.
# Run by src/fcmp_pp/fcmp_pp_rust/CMakeLists.txt; needs only sh, sed, awk, grep, find, cmp.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
rust="$here/../src/fcmp_pp/fcmp_pp_rust"
bases="$rust/patched/bases.txt"
lock="$rust/Cargo.lock"
fail() { echo "check-vendor-rev: FAIL: $*" >&2; exit 1; }

[ -f "$bases" ] || fail "missing $bases"
[ -d "$rust/vendor" ] || fail "missing $rust/vendor (run scripts/vendor.sh)"

mo_rev=$(awk '$1 == "monero-oxide" { print $4 }' "$bases")
dfg_ver=$(awk '$1 == "dalek-ff-group" { print $3 }' "$bases")
[ -n "$mo_rev" ] || fail "no monero-oxide line in $bases"
[ -n "$dfg_ver" ] || fail "no dalek-ff-group line in $bases"
mx_ver=$(awk '$1 == "multiexp" { print $3 }' "$bases")
[ -n "$mx_ver" ] || fail "no multiexp line in $bases"

# 1. Cargo.lock pins every monero-oxide git package at the patched tree's base revision.
revs=$(sed -n 's|^source = "git+https://github.com/monero-oxide/monero-oxide[^#]*#\([0-9a-f]*\)"$|\1|p' "$lock" | sort -u)
[ -n "$revs" ] || fail "no monero-oxide git packages in Cargo.lock"
[ "$revs" = "$mo_rev" ] || fail "Cargo.lock pins monero-oxide at $revs; patched/monero-oxide is based on $mo_rev"

# 2. Each vendored monero-oxide crate matches its copy in patched/monero-oxide.
pkgs=$(awk '
  /^\[\[package\]\]/ { name = ""; ver = "" }
  /^name = / { name = $3 }
  /^version = / { ver = $3 }
  /^source = "git\+https:\/\/github.com\/monero-oxide\/monero-oxide/ { gsub(/"/, "", name); gsub(/"/, "", ver); print name "-" ver "|" name }
' "$lock")
n=0
for p in $pkgs; do
  dir=${p%%|*}
  name=${p#*|}
  v="$rust/vendor/$dir"
  [ -d "$v" ] || fail "vendor/$dir missing (run scripts/vendor.sh)"
  m=$(grep -rlx --include=Cargo.toml "name = \"$name\"" "$rust/patched/monero-oxide" || true)
  [ "$(printf '%s\n' "$m" | grep -c .)" = 1 ] || fail "found $(printf '%s\n' "$m" | grep -c .) crates named $name in patched/monero-oxide"
  src=$(dirname "$m")
  files=$(cd "$v" && find . -type f ! -name .cargo-checksum.json ! -name Cargo.toml ! -name Cargo.toml.orig ! -name .cargo_vcs_info.json ! -name Cargo.lock)
  [ -n "$files" ] || fail "vendor/$dir is empty"
  for f in $files; do
    cmp -s "$v/$f" "$src/$f" || fail "vendor/$dir/${f#./} differs from patched/monero-oxide (or is missing there)"
  done
  n=$((n + 1))
done
[ "$n" -gt 0 ] || fail "no vendored monero-oxide crates found"

# 3. dalek-ff-group is the patched path package, at the base version.
dfg=$(awk -v ver="$dfg_ver" 'BEGIN { RS = "" }
  index($0, "\nname = \"dalek-ff-group\"\n") && index($0, "\nversion = \"" ver "\"") { print (index($0, "\nsource = ") ? "registry" : "path") }
' "$lock")
[ "$dfg" = "path" ] || fail "Cargo.lock: dalek-ff-group $dfg_ver is '$dfg', expected the patched path package"
grep -qx "version = \"$dfg_ver\"" "$rust/patched/dalek-ff-group/Cargo.toml" || fail "patched/dalek-ff-group is not version $dfg_ver"

# 4. Every multiexp in Cargo.lock is the patched path package, at the base version; none is vendored.
mx=$(awk 'BEGIN { RS = ""; FS = "\n" }
  {
    name = 0; ver = ""; src = "path"
    for (i = 1; i <= NF; i++) {
      if ($i == "name = \"multiexp\"") name = 1
      if ($i ~ /^version = "/) { ver = $i; gsub(/^version = "|"$/, "", ver) }
      if ($i ~ /^source = /) src = "registry"
    }
    if (name) print ver "/" src
  }
' "$lock")
[ "$mx" = "$mx_ver/path" ] || fail "Cargo.lock: multiexp is '$(echo $mx)', expected only $mx_ver/path, the patched path package"
grep -qx "version = \"$mx_ver\"" "$rust/patched/multiexp/Cargo.toml" || fail "patched/multiexp is not version $mx_ver"
for d in "$rust"/vendor/multiexp-*; do
  if [ -e "$d" ]; then fail "vendor/${d##*/} exists; the build must use patched/multiexp only"; fi
done

echo "check-vendor-rev: ok (monero-oxide $mo_rev: $n vendored crates match patched/monero-oxide; dalek-ff-group $dfg_ver and multiexp $mx_ver from patched/)"
