#!/bin/sh
# The master runs as the user "fafnirx" (uid 1000), but a folder mounted at /data or /config is often owned by root: a cloud
# platform or a plain `docker run -v` creates it that way. Started as root, this fixes the ownership of those folders (only
# when it is wrong, so a big data folder is not walked at every start) and then drops to fafnirx. Started as another user
# (compose `user:`), it changes nothing and runs the master as that user.
set -eu

if [ "$(id -u)" = 0 ]; then
  for dir in "${FAFNIRX_DATA:-/data}" /config; do
    [ -d "$dir" ] || continue
    if [ "$(stat -c %u "$dir")" != 1000 ]; then
      chown -R 1000:1000 "$dir"
    fi
  done
  exec setpriv --reuid=1000 --regid=1000 --init-groups -- "$@"
fi
exec "$@"
