#!/usr/bin/env bash
set -euo pipefail

# The Azure Ubuntu mirror can stall Actions runners for many minutes. Keep the
# runner's suites/components, but fetch Ubuntu packages from the official mirror.
shopt -s nullglob
for source in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources \
  /etc/apt/apt-mirrors.txt /etc/apt/mirrors/*.list; do
  if [[ -f "$source" ]]; then
    sed -i -E 's#https?://azure\.archive\.ubuntu\.com/ubuntu#https://archive.ubuntu.com/ubuntu#g' "$source"
  fi
done

apt_options=(-o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 -o Acquire::Retries=2)
apt-get "${apt_options[@]}" update
apt-get "${apt_options[@]}" install -y "$@"
