#!/usr/bin/env bash
# Checks the layout rules of the Rust code (CI runs it): tests live apart from the logic, and no logic file grows large.
#   1. no inline `#[cfg(test)] mod name { ... }` block in a `src/` file;
#   2. every `#[path = "..."]` test module points to a file that exists;
#   3. no logic file (tests excluded) is over MAX lines: split it into a folder (mod.rs plus files by job).
# It also prints the size of the logic and of the tests.
set -euo pipefail
cd "$(dirname "$0")/.."

MAX=600

fail=0
files=$(git ls-files --cached --others --exclude-standard '*.rs' | while read -r f; do [ -f "$f" ] && echo "$f"; done)
logic=$(echo "$files" | grep -v '/tests/' || true)
tests=$(echo "$files" | grep '/tests/' || true)

for f in $logic; do
  if grep -Pzo '#\[cfg\(test\)\]\s*\n\s*mod [a-z_0-9]+ *\{' "$f" > /dev/null 2>&1; then
    echo "inline test block in $f: move it to tests/unit/ and use #[path]"; fail=1
  fi
  while IFS= read -r p; do
    target="$(dirname "$f")/$p"
    [ -f "$target" ] || { echo "$f: #[path = \"$p\"] points to a missing file"; fail=1; }
  done < <(grep -oP '#\[path = "\K[^"]+' "$f" || true)
  n=$(wc -l < "$f")
  if [ "$n" -gt "$MAX" ]; then
    echo "$f has $n lines (max $MAX): split it"; fail=1
  fi
done

summary="Rust logic: $(echo "$logic" | xargs cat | wc -l) lines, tests: $(echo "$tests" | xargs cat | wc -l) lines"
echo "$summary"
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || echo "$summary" >> "$GITHUB_STEP_SUMMARY"
exit $fail
