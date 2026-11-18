# Print whether this node serves mm's shared storage and whether it keeps
# files there, without changing anything.
# Arguments: STORAGE_ROOT
set -eu
root=$1
serves=no files=no
if [ -f /etc/exports.d/mm-storage.exports ]; then serves=yes; fi
# Directories alone are no data: a volume nobody wrote to.
if [ -d "$root" ] && [ -n "$(find "$root" -mindepth 1 ! -type d -print -quit 2>/dev/null)" ]; then
    files=yes
fi
printf 'serves=%s files=%s\n' "$serves" "$files"
