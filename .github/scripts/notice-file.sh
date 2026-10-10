#!/usr/bin/env bash
# Usage: notice-file.sh <title> <file>
# Posts the first lines of a text file as a workflow notice (annotation). Used to show build
# outputs in the checks API, where the job log can't be read from outside GitHub.
set -u
title=$1
file=$2
[ -f "$file" ] || exit 0
escape() { sed -e 's/%/%25/g' -e 's/\r/%0D/g' | awk '{ printf "%s%%0A", $0 }'; }
echo "::notice title=${title}::$(head -n 60 "$file" | escape | head -c 3500)"
