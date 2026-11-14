#!/usr/bin/env bash
# Usage: report-failure.sh <title> <log-file>
#
# Posts the errors in a failed build's log as a workflow error annotation. The full log is in the
# job's output; the annotation is also visible through the checks API, which is handy when only the
# API can be reached. Prints nothing and exits 0 if the log is missing.
set -u

title=${1:-build failed}
log=${2:-}
[ -n "$log" ] && [ -f "$log" ] || exit 0

# Annotation messages must escape '%' and line breaks.
escape() {
  sed -e 's/%/%25/g' -e 's/\r/%0D/g' | awk '{ print; print "%0A" }' | tr -d '\n'
}

errors=$(grep -E "(^|: )error(\[|:)" "$log" | head -n 25 | escape | head -c 3500)
if [ -n "$errors" ]; then
  message=$errors
else
  message=$(tail -n 40 "$log" | escape | head -c 3500)
fi
echo "::error title=${title}::${message}"
