#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORKFLOW="$ROOT/.github/workflows/release-assemble.yml"
CI_WORKFLOW="$ROOT/.github/workflows/ci.yml"

die() {
  printf 'release workflow test: %s\n' "$*" >&2
  exit 1
}

[[ -f "$WORKFLOW" ]] || die 'release workflow is missing'
[[ -f "$CI_WORKFLOW" ]] || die 'CI workflow is missing'

trigger_block="$(sed -n '/^on:$/,/^permissions:$/p' "$WORKFLOW")"
grep -Fq '  workflow_dispatch:' <<< "$trigger_block" || die 'release workflow must retain an explicit manual trigger'
grep -Fq '    inputs:' <<< "$trigger_block" || die 'manual release trigger must require a tag input'
grep -Fq '      tag:' <<< "$trigger_block" || die 'manual release trigger is missing its tag input'
grep -Fq '        required: true' <<< "$trigger_block" || die 'manual release tag input must be required'
grep -Fq '        type: string' <<< "$trigger_block" || die 'manual release tag input must be a string'
grep -Fq "      - 'v*'" <<< "$trigger_block" || die 'release workflow is missing its version tag trigger'

for workflow in "$CI_WORKFLOW" "$WORKFLOW"; do
  top_level_env="$(awk '
    /^env:$/ { active = 1; next }
    active && /^[^[:space:]]/ { active = 0 }
    active { print }
  ' "$workflow")"
  if grep -Fq '${{ runner.' <<< "$top_level_env"; then
    die "workflow-level env uses the unavailable runner context: $workflow"
  fi
  if grep -Fq 'macos-13' "$workflow"; then
    die "workflow uses the retired macos-13 runner: $workflow"
  fi
  if grep -Eq 'uses: actions/(checkout|upload-artifact|download-artifact)@v4' "$workflow"; then
    die "workflow uses a retired Node 20 action release: $workflow"
  fi
done

condition="if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v') && github.repository == 'NeotaskInc/neomax-orchestrator-rust'"
grep -Fq "$condition" "$WORKFLOW" || die 'publish job is missing the exact repository and tag gate'

release_ref_block="$(sed -n '/^  release_ref:/,/^  quality:/p' "$WORKFLOW")"
grep -Fq 'fetch-depth: 0' <<< "$release_ref_block" || die 'release ref validation must fetch complete tag history'
grep -Fq 'fetch-tags: true' <<< "$release_ref_block" || die 'release ref validation must fetch tags'
grep -Fq 'workflow_dispatch)' <<< "$release_ref_block" || die 'release ref validation must handle manual dispatch explicitly'
grep -Fq 'workflow_dispatch must be run against a tag' <<< "$release_ref_block" || die 'manual dispatch must reject branch refs'
grep -Fq 'DISPATCH_TAG' <<< "$release_ref_block" || die 'manual dispatch must validate its tag input'
grep -Fq 'tag input must match the selected tag ref' <<< "$release_ref_block" || die 'manual dispatch tag input must match the selected ref'
grep -Fq 'git show-ref --verify --quiet' <<< "$release_ref_block" || die 'release ref validation must require an existing tag'
grep -Fq 'git rev-parse --verify "${tag}^{commit}"' <<< "$release_ref_block" || die 'release ref validation must resolve annotated or lightweight tags'
grep -Fq 'git rev-parse --verify HEAD' <<< "$release_ref_block" || die 'release ref validation must inspect checked-out HEAD'
grep -Fq '[[ "$tag_sha" == "$head_sha" ]]' <<< "$release_ref_block" || die 'release ref validation must require tag and HEAD identity'
grep -Fq 'printf '\''tag=%s\n'\'' "$tag"' <<< "$release_ref_block" || die 'release ref validation must export the verified tag'
grep -Fq 'printf '\''sha=%s\n'\'' "$tag_sha"' <<< "$release_ref_block" || die 'release ref validation must export the exact tag commit SHA'

grep -A3 -F '  quality:' "$WORKFLOW" | grep -Fq 'needs: release_ref' || die 'quality must wait for release ref validation'
grep -A3 -F '  msrv:' "$WORKFLOW" | grep -Fq 'needs: release_ref' || die 'MSRV must wait for release ref validation'
ci_gate_block="$(sed -n '/^  ci_gate:/,/^  quality:/p' "$WORKFLOW")"
grep -Fq 'needs: release_ref' <<< "$ci_gate_block" || die 'CI gate must wait for release ref validation'
grep -Fq 'actions: read' <<< "$ci_gate_block" || die 'CI gate must have read-only Actions access'
grep -Fq 'RELEASE_SHA: ${{ needs.release_ref.outputs.sha }}' <<< "$ci_gate_block" || die 'CI gate must use the resolved tag commit SHA'
grep -Fq 'dist/verify-ci-run.sh' <<< "$ci_gate_block" || die 'CI gate must verify a successful exact-SHA Rust CI run'
grep -Fq -- '--branch main' <<< "$ci_gate_block" || die 'CI gate must require the exact SHA to pass on main'
grep -Fq 'needs: [release_ref, ci_gate, quality, msrv]' "$WORKFLOW" || die 'release metadata must depend on the exact-SHA CI gate'

publish_block="$(sed -n '/^  publish:/,$p' "$WORKFLOW")"
grep -Fq '      contents: write' <<< "$publish_block" || die 'publish job must own the write scope'
! grep -Fq 'RELEASE_REPOSITORY' <<< "$publish_block" || die 'publish job must not override the checked-out repository'
grep -Fq 'RELEASE_TAG: ${{ needs.release-metadata.outputs.tag }}' <<< "$publish_block" || die 'publish must use the validated release tag'
grep -Fq 'git fetch --force --tags' <<< "$publish_block" || die 'publish must fetch the release tag before verification'
grep -Fq 'git rev-parse --verify "${RELEASE_TAG}^{commit}"' <<< "$publish_block" || die 'publish must resolve the release tag commit'
grep -Fq '[[ "$tag_sha" == "$head_sha" ]]' <<< "$publish_block" || die 'publish must require the tag to resolve to checked-out HEAD'
grep -Fq 'RELEASE_SHA: ${{ needs.release-metadata.outputs.sha }}' <<< "$publish_block" || die 'publish must pass the peeled tag commit SHA to the transaction'
grep -Fq 'dist/publish-release.sh' <<< "$publish_block" || die 'publish must use the verified draft-first release transaction'
grep -Fq -- '--expected-sha "$RELEASE_SHA"' <<< "$publish_block" || die 'publish transaction must receive the exact release SHA'
! grep -Fq -- '--generate-notes' <<< "$publish_block" || die 'release publishing must use only the reviewed release notes'

publication_script="$ROOT/dist/publish-release.sh"
grep -Fq -- '--draft=true' "$publication_script" || die 'reruns must move an existing release to draft before mutation'
grep -Fq 'release delete-asset' "$publication_script" || die 'reruns must remove stale release assets'
grep -Fq 'verify-release-assets.sh' "$publication_script" || die 'publication must verify downloaded release contents'
grep -Fq -- '--draft=false' "$publication_script" || die 'publication must flip the release public only after verification'
grep -Fq 'NeotaskInc/neomax-orchestrator-rust' "$publication_script" || die 'publication script must enforce the organization repository boundary'

ci_verifier="$ROOT/dist/verify-ci-run.sh"
grep -Fq 'head_sha=$sha' "$ci_verifier" || die 'CI lookup must query the exact tag commit SHA'
grep -Fq 'run.get("head_sha") == sha' "$ci_verifier" || die 'CI response validation must bind success to the exact tag commit SHA'
grep -Fq 'run.get("conclusion") == "success"' "$ci_verifier" || die 'CI response validation must require success'
grep -Fq 'run.get("event") == "push"' "$ci_verifier" || die 'CI response validation must require a push run'
grep -Fq 'run.get("head_branch") == branch' "$ci_verifier" || die 'CI response validation must require the selected branch'
grep -Fq 'actions/runs/$run_id/jobs?per_page=100' "$ci_verifier" || die 'CI verifier must inspect the complete job matrix'
grep -Fq '"native-tests (windows-latest)"' "$ci_verifier" || die 'CI verifier must require the Windows native test job'
grep -Fq '"native-package (macos-14, aarch64-apple-darwin)"' "$ci_verifier" || die 'CI verifier must require every native package job'
grep -Fq '"cross-package (aarch64-unknown-linux-musl)"' "$ci_verifier" || die 'CI verifier must require every cross-package job'

temporary="$(mktemp -d "${TMPDIR:-/tmp}/neomax-workflow-test.XXXXXX")"
trap 'rm -rf "$temporary"' EXIT
fake_bin="$temporary/bin"
mkdir -p "$fake_bin"
printf '%s\n' \
  '#!/usr/bin/env bash' \
  'case "$*" in' \
  '  *"/jobs"*) cat "$FAKE_CI_JOBS_RESPONSE" ;;' \
  '  *) cat "$FAKE_CI_RESPONSE" ;;' \
  'esac' > "$fake_bin/gh"
chmod 0755 "$fake_bin/gh"
ci_sha='0123456789abcdef0123456789abcdef01234567'
success_response="$temporary/success.json"
printf '%s\n' "{\"workflow_runs\":[{\"id\":20,\"head_sha\":\"$ci_sha\",\"status\":\"completed\",\"conclusion\":\"success\",\"path\":\".github/workflows/ci.yml\",\"event\":\"push\",\"head_branch\":\"main\",\"run_number\":20,\"html_url\":\"https://example.invalid/actions/20\"}]}" > "$success_response"
success_jobs="$temporary/success-jobs.json"
python3 - "$success_jobs" <<'PY'
import json
import pathlib
import sys

names = [
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
]
payload = {
    "total_count": len(names),
    "jobs": [
        {"name": name, "status": "completed", "conclusion": "success"}
        for name in names
    ],
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(payload), encoding="utf-8")
PY
PATH="$fake_bin:$PATH" FAKE_CI_RESPONSE="$success_response" FAKE_CI_JOBS_RESPONSE="$success_jobs" bash "$ci_verifier" --repository NeotaskInc/neomax-orchestrator-rust --sha "$ci_sha" >/dev/null

incomplete_jobs="$temporary/incomplete-jobs.json"
python3 - "$success_jobs" "$incomplete_jobs" <<'PY'
import json
import pathlib
import sys

payload = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
payload["jobs"].pop()
payload["total_count"] = len(payload["jobs"])
pathlib.Path(sys.argv[2]).write_text(json.dumps(payload), encoding="utf-8")
PY
if PATH="$fake_bin:$PATH" FAKE_CI_RESPONSE="$success_response" FAKE_CI_JOBS_RESPONSE="$incomplete_jobs" bash "$ci_verifier" --repository NeotaskInc/neomax-orchestrator-rust --sha "$ci_sha" >/dev/null 2>&1; then
  die 'CI verifier accepted an incomplete job matrix'
fi

wrong_sha_response="$temporary/wrong-sha.json"
printf '%s\n' '{"workflow_runs":[{"head_sha":"fedcba9876543210fedcba9876543210fedcba98","status":"completed","conclusion":"success","path":".github/workflows/ci.yml","event":"push","head_branch":"main","run_number":21,"html_url":"https://example.invalid/actions/21"}]}' > "$wrong_sha_response"
if PATH="$fake_bin:$PATH" FAKE_CI_RESPONSE="$wrong_sha_response" bash "$ci_verifier" --repository NeotaskInc/neomax-orchestrator-rust --sha "$ci_sha" >/dev/null 2>&1; then
  die 'CI verifier accepted a successful run for the wrong commit SHA'
fi

failed_response="$temporary/failed.json"
printf '%s\n' "{\"workflow_runs\":[{\"head_sha\":\"$ci_sha\",\"status\":\"completed\",\"conclusion\":\"failure\",\"path\":\".github/workflows/ci.yml\",\"event\":\"push\",\"head_branch\":\"main\",\"run_number\":22,\"html_url\":\"https://example.invalid/actions/22\"}]}" > "$failed_response"
if PATH="$fake_bin:$PATH" FAKE_CI_RESPONSE="$failed_response" bash "$ci_verifier" --repository NeotaskInc/neomax-orchestrator-rust --sha "$ci_sha" >/dev/null 2>&1; then
  die 'CI verifier accepted a failed run'
fi

can_publish() {
  local event="$1"
  local ref="$2"
  local repository="$3"
  [[ "$event" == push && "$ref" == refs/tags/v* && "$repository" == NeotaskInc/neomax-orchestrator-rust ]]
}

if can_publish push refs/tags/v0.1.0 example-owner/neomax-orchestrator-rust; then
  die 'personal private repository passed the publication gate'
fi
if can_publish push refs/tags/v0.1.0 NeotaskInc/neomax-orchestrator-rust; then
  :
else
  die 'verified organization tag did not pass the publication gate'
fi
if can_publish workflow_dispatch refs/tags/v0.1.0 NeotaskInc/neomax-orchestrator-rust; then
  die 'manual dispatch passed the publication gate'
fi
if can_publish push refs/heads/main NeotaskInc/neomax-orchestrator-rust; then
  die 'branch push passed the publication gate'
fi

printf '%s\n' 'release workflow boundary checks passed'
