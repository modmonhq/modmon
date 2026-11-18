# Print the member record of a managed store (or null) and whether its Raft
# data is on disk. Arguments: STORE_ID.
set -eu
record=/etc/mm-openbao/$1/member.json
data=false
if [ -f "/var/lib/mm-openbao/$1/data/vault.db" ]; then
    data=true
fi
printf '{"data":%s,"record":' "$data"
if [ -f "$record" ]; then
    cat "$record"
else
    printf 'null'
fi
printf '}\n'
