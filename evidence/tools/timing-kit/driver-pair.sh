#!/bin/bash
# coordinator driver, runs ON the test box: the extra pair after the B N N B kit (optimizer research 5dca89a, prediction 12), arms N (.9 22c1f9ee7)
# then B (.8 e7bae3ecf), numbered runs 5 and 6, plus a top-hash check against the reference node after each run (get_height, 60 s timeout, 3 tries).
# Changes from the B N N B driver: thrstat fixed (it used kill -0, EPERM across users, so it never sampled); after passing, 60 s grace with
# the node running, then monerod stops first and the sidecar 90 s later, so the last blocks and samples reach the hub. Each run: scratch lmdb from the local master copy of the popped
# file (height 3,102,000), sha256 check, /opt/monero/current -> the arm, the unchanged unit, stop once the log Height
# passes 3,103,451. No pidstat/perf on this box: a /proc per-thread sampler every 5 s (thrstat) and cpu MHz every 5 min instead. At the end the tip DB goes back.
# A scratch lmdb carries the marker file .bpp-scratch; only a directory with that marker is ever removed.
set -u
DD=/var/lib/monero/testnet; M=/var/lib/monero/bpp-master/data.mdb; O=$HOME/kit;REFA=${REFA:?set REFA to the reference node RPC, e.g. http://host:28081}; L=$DD/bitmonero.log
TIP=$DD/lmdb.dato7-tip-20261117g; STOP_AT=3103452; SHA=16c687f9f27b791e3414c0303603480c5a85c80b465e6d5af0a81cc7d974fbfe
REL=/opt/monero/v0.19.0.0-beta.3.1-monerodato.7
lg() { echo "$(date -u +%FT%TZ) $*" | tee -a "$O/driver.log"; }
abort() { lg "DOT9C_ABORT $*"; exit 1; }
height() { sudo -n tail -c 3000000 "$L" | awk -v s="$1" '$0 >= s' | grep -a "Height:" | tail -1 | grep -oE "Height: [0-9]+" | grep -oE "[0-9]+$"; }
rpc_wait() { for i in $(seq 1 90); do r=$(curl -s -m 5 http://127.0.0.1:28081/get_info | grep -oE '"(height|version)": *"?[^,"]+' | tr -d ' \n'); [ -n "$r" ] && { echo "$r"; return 0; }; sleep 2; done; return 1; }
rm_scratch() { if sudo -n test -e "$DD/lmdb"; then sudo -n test -e "$DD/lmdb/.bpp-scratch" || abort "lmdb has no scratch marker: not removed"; sudo -n test -e "$TIP/data.mdb" || abort "tip DB missing"; sudo -n rm -rf "$DD/lmdb"; fi; }
# /proc/<pid>/task/*/stat is world-readable; f[] starts at stat field 3, so utime (14) is f[11], stime (15) f[12], processor (39) f[36].
thrstat() { local pid=$1 t x c f; while [ -d "/proc/$pid/task" ]; do echo "== $(date -u +%FT%T.%3NZ)"; for t in /proc/$pid/task/*; do x=$(cat "$t/stat" 2>/dev/null) || continue; c=${x#*(}; c=${c%)*}; read -ra f <<< "${x##*) }"; echo "${t##*/} ${c// /_} utime ${f[11]} stime ${f[12]} cpu ${f[36]}"; done; sleep 5; done; }
top_hash() { local th hq mine refa; for _ in 1 2 3; do th=$(curl -s -m 60 http://127.0.0.1:28081/get_height | grep -oE '"height": *[0-9]+' | grep -oE '[0-9]+$'); [ -n "$th" ] && break; sleep 5; done
  [ -n "$th" ] || { echo "top block none hash  the reference node  match=NO (get_height gave no answer in 3 tries)"; return; }; th=$((th-1))
  hq='{"jsonrpc":"2.0","id":0,"method":"get_block_header_by_height","params":{"height":'$th'}}'
  mine=$(curl -s -m 60 http://127.0.0.1:28081/json_rpc -d "$hq" | grep -oE '"hash": *"[0-9a-f]{64}"' | grep -oE '[0-9a-f]{64}')
  refa=$(curl -s -m 20 $REFA/json_rpc -d "$hq" | grep -oE '"hash": *"[0-9a-f]{64}"' | grep -oE '[0-9a-f]{64}')
  if [ -n "$mine" ] && [ "$mine" = "$refa" ]; then echo "top block $th hash $mine the reference node $refa match=yes"; else echo "top block $th hash $mine the reference node $refa match=NO"; fi; }
lg "driver start; master $(sudo -n stat -c %s "$M") bytes"
sudo -n systemctl stop sampler; sudo -n systemctl stop monerod
sudo -n test -e "$TIP" && abort "$TIP exists already"
sudo -n mv "$DD/lmdb" "$TIP" || abort "could not move the tip DB aside"
lg "tip DB moved to $TIP"
i=4
for a in N B; do
  i=$((i+1)); R="$O/run$i-$a"
  rm_scratch
  sudo -n -u monero mkdir -m 775 "$DD/lmdb" && sudo -n -u monero touch "$DD/lmdb/.bpp-scratch" || abort "mkdir lmdb"
  lg "run $i arm $a: copy start"
  sudo -n -u monero cp "$M" "$DD/lmdb/data.mdb" || abort "copy"
  [ "$(sudo -n sha256sum "$DD/lmdb/data.mdb" | cut -d' ' -f1)" = "$SHA" ] || abort "sha256 of the copy"
  sudo -n ln -sfn /opt/monero/dot9-$a /opt/monero/current
  S=$(date -u '+%F %T'); sudo -n systemctl start monerod || abort "start monerod"
  v=$(rpc_wait) || abort "RPC did not answer"
  sudo -n systemctl start sampler
  p=$(systemctl show -p MainPID --value monerod)
  lg "run $i arm $a: monerod started $S UTC pid $p exe $(sha256sum /opt/monero/current/monerod | cut -c1-12) $v"
  thrstat "$p" > "$R.thrstat.txt" 2>&1 &
  TS=$!
  n=0; t0=$(date +%s)
  while :; do
    sleep 30; n=$((n+1)); h=$(height "$S")
    [ "$(systemctl is-active monerod)" = active ] || abort "monerod not active in run $i"
    if [ $((n % 10)) -eq 0 ]; then
      { echo "== $(date -u +%FT%TZ) height ${h:-none} load $(cut -d' ' -f1-3 /proc/loadavg) throttle $(cat /sys/devices/system/cpu/cpu[0-1]/thermal_throttle/core_throttle_count | tr '\n' ' ')"
        grep -E '^cpu MHz' /proc/cpuinfo; } >> "$R.clock.txt"
    fi
    [ -n "$h" ] && [ "$h" -ge $STOP_AT ] && break
    [ $(( $(date +%s) - t0 )) -gt 12600 ] && abort "run $i over 3.5 h at height ${h:-none}"
  done
  cpu=$(awk '{print $14+$15}' /proc/$p/stat); rss=$(grep RssAnon /proc/$p/status | awk '{print $2}')
  lg "run $i arm $a: passed Height $h; node cpu ticks $cpu; RssAnon $rss kB"
  lg "run $i arm $a: $(top_hash)"
  vf=$(sudo -n awk -v s="$S" '$0 >= s' "$L" | grep -c -E "Failed to verify|verification failed|Maximum packet size")
  lg "run $i arm $a: verify/max-packet lines $vf; grace 60 s before the stop"
  sleep 60
  sudo -n systemctl stop monerod; lg "run $i arm $a: monerod stopped; sidecar stops in 90 s"
  sleep 90; sudo -n systemctl stop sampler
  kill $TS 2>/dev/null
  lg "run $i arm $a: stopped"
done
rm_scratch
sudo -n mv "$TIP" "$DD/lmdb" || abort "could not move the tip DB back"
sudo -n ln -sfn "$REL" /opt/monero/current
sudo -n systemctl start monerod; v=$(rpc_wait) || abort "RPC did not answer after the restore"
sudo -n systemctl start sampler
lg "tip DB back, release binary, $v"
lg "DOT9C_ALL_DONE"
