#!/bin/sh
# telnet-run.sh <host> <command>
HOST=${1:-192.168.1.154}
shift || true
{ printf '%s\n' "$*"; printf 'exit\n'; } | timeout 60 nc "$HOST" 2323
