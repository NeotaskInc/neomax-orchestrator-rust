#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

repository=""
sha=""
workflow="ci.yml"
branch="main"
while (($#)); do
  case "$1" in
    --repository)
      (($# >= 2)) || die '--repository requires OWNER/REPOSITORY'
      repository="$2"
      shift 2
      ;;
    --sha)
      (($# >= 2)) || die '--sha requires a commit SHA'
      sha="$2"
      shift 2
      ;;
    --workflow)
      (($# >= 2)) || die '--workflow requires a workflow file name'
      workflow="$2"
      shift 2
      ;;
    --branch)
      (($# >= 2)) || die '--branch requires a branch name'
      branch="$2"
      shift 2
      ;;
    --help|-h)
      printf '%s\n' 'usage: verify-ci-run.sh --repository OWNER/REPOSITORY --sha SHA [--workflow ci.yml] [--branch main]'
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || die 'repository must be OWNER/REPOSITORY'
[[ "$sha" =~ ^[0-9a-f]{40}$ ]] || die 'sha must be a full lowercase commit SHA'
[[ "$workflow" =~ ^[A-Za-z0-9_.-]+$ ]] || die 'workflow must be a file name'
[[ "$branch" =~ ^[A-Za-z0-9._/-]+$ && "$branch" != ../* && "$branch" != */../* && "$branch" != */.. ]] || die 'branch name is invalid'
require_command gh
require_command python3

runs="$(mktemp "${TMPDIR:-/tmp}/neomax-ci-runs.XXXXXX")"
jobs="$(mktemp "${TMPDIR:-/tmp}/neomax-ci-jobs.XXXXXX")"
trap 'rm -f "$runs" "$jobs"' EXIT
gh api "repos/$repository/actions/workflows/$workflow/runs?head_sha=$sha&status=success&per_page=100" > "$runs"

match="$(python3 - "$runs" "$sha" "$workflow" "$branch" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
sha = sys.argv[2]
workflow = sys.argv[3]
branch = sys.argv[4]
matches = [
    run
    for run in payload.get("workflow_runs", [])
    if run.get("head_sha") == sha
    and run.get("status") == "completed"
    and run.get("conclusion") == "success"
    and run.get("path") == f".github/workflows/{workflow}"
    and run.get("event") == "push"
    and run.get("head_branch") == branch
]
if not matches:
    raise SystemExit(f"no successful Rust CI run exists for exact commit {sha}")
match = max(matches, key=lambda run: run.get("run_number", 0))
print(f'{match["id"]}\t{match["html_url"]}')
PY
)"
IFS=$'\t' read -r run_id run_url <<< "$match"
[[ "$run_id" =~ ^[0-9]+$ && "$run_url" == https://* ]] || die 'successful CI run metadata is invalid'
gh api "repos/$repository/actions/runs/$run_id/jobs?per_page=100" > "$jobs"

python3 - "$jobs" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
jobs = payload.get("jobs", [])
expected = {
    "quality",
    "msrv",
    "native-tests (macos-15-intel)",
    "native-tests (macos-14)",
    "native-tests (windows-latest)",
    "native-package (ubuntu-24.04, x86_64-unknown-linux-gnu)",
    "native-package (macos-15-intel, x86_64-apple-darwin)",
    "native-package (macos-14, aarch64-apple-darwin)",
    "native-package (windows-latest, x86_64-pc-windows-msvc)",
    "cross-package (aarch64-unknown-linux-gnu)",
    "cross-package (x86_64-unknown-linux-musl)",
    "cross-package (aarch64-unknown-linux-musl)",
}
for job in jobs:
    name = job.get("name", "")
    if name not in expected:
        raise SystemExit(f"unexpected Rust CI job: {name or '<unnamed>'}")
    if job.get("status") != "completed" or job.get("conclusion") != "success":
        raise SystemExit(f"Rust CI job did not succeed: {name}")
received = {job.get("name", "") for job in jobs}
if received != expected or len(jobs) != len(expected):
    missing = sorted(expected - received)
    raise SystemExit(f"Rust CI job matrix is incomplete; missing jobs: {missing}")
PY

printf '%s\n' "$run_url"
