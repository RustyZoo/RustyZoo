# Splits a long list of independent items over parallel CI jobs. Source it (`. "$(dirname "$0")/shard.sh"`).
#
# SHARD=<i>/<n> (1 <= i <= n < 1000) keeps the items i, i+n, i+2n... of the list (counted from 1), so n jobs
# with i = 1..n run every item once; unset or empty keeps them all. Anything else is refused (exit 2).
#   shard_has <k>            true when the item k belongs to this shard
#   shard_done <what> <kept> <total>   prints `shard i/n: <kept> of <total> <what>`; fails when a shard keeps nothing
if [ -n "${SHARD:-}" ]; then
  if ! [[ $SHARD =~ ^[1-9][0-9]{0,2}/[1-9][0-9]{0,2}$ ]] || [ "${SHARD%/*}" -gt "${SHARD#*/}" ]; then
    echo "FAILED: SHARD='$SHARD' is not <i>/<n> with 1 <= i <= n < 1000" >&2
    exit 2
  fi
fi
shard_has() { [ -z "${SHARD:-}" ] || [ $((($1 - 1) % ${SHARD#*/} + 1)) = "${SHARD%/*}" ]; }
shard_done() {
  [ -n "${SHARD:-}" ] || return 0
  echo "shard $SHARD: $2 of $3 $1"
  [ "$2" -gt 0 ] || { echo "FAILED: shard $SHARD is empty" >&2; return 1; }
}
