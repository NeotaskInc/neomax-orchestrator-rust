#!/bin/sh
set -eu

dash_pattern=$(printf '\342\200\224|\342\200\223')
matches=$(
  find . \( -path './target' -o -path './.git' \) -prune -o \
    -type f -name '*.md' -exec grep -nH -E "$dash_pattern" {} + || true
)
if [ -n "$matches" ]; then
  printf '%s\n' "$matches" >&2
  printf '%s\n' 'Markdown must use ASCII punctuation; em and en dashes are not allowed.' >&2
  exit 1
fi
