#!/usr/bin/env bash
# Non-destructive Zed setup for tc_r1. Run manually; tests never invoke this.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
ZED_CONFIG="${HOME}/.config/zed"

echo "==> Building tc, tc-lsp, and r1-editor (offline release)"
CARGO_NET_OFFLINE=true cargo build --release --offline \
  -p tc-cli -p tc-lsp -p tc-editor --manifest-path "${ROOT}/Cargo.toml"

mkdir -p "${BIN_DIR}"
for name in tc tc-lsp r1-editor; do
  ln -sfn "${ROOT}/target/release/${name}" "${BIN_DIR}/${name}"
  echo "    linked ${BIN_DIR}/${name}"
done

mkdir -p "${ZED_CONFIG}"
if [ -f "${ZED_CONFIG}/settings.json" ]; then
  backup="${ZED_CONFIG}/settings.json.tc-r1-backup.$(date +%Y%m%d%H%M%S)"
  cp "${ZED_CONFIG}/settings.json" "${backup}"
  echo "==> Backed up existing Zed settings to ${backup}"
fi

python3 - "${ZED_CONFIG}/settings.json" "${ROOT}/editors/zed/settings.json" <<'PY'
import json
import sys

target_path, source_path = sys.argv[1], sys.argv[2]
try:
    with open(target_path, "r", encoding="utf-8") as handle:
        target = json.load(handle)
except (FileNotFoundError, json.JSONDecodeError):
    target = {}
with open(source_path, "r", encoding="utf-8") as handle:
    source = json.load(handle)

def merge(base, overlay):
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            merge(base[key], value)
        else:
            base[key] = value

merge(target, source)
with open(target_path, "w", encoding="utf-8") as handle:
    json.dump(target, handle, indent=2, sort_keys=False)
    handle.write("\n")
PY

echo "==> Wrote offline zero-AI settings to ${ZED_CONFIG}/settings.json"

python3 - "${ZED_CONFIG}/tasks.json" "${ROOT}/editors/zed/tasks.json" <<'PYTASKS'
import json
import sys

target_path, source_path = sys.argv[1], sys.argv[2]
try:
    with open(target_path, "r", encoding="utf-8") as handle:
        target = json.load(handle)
    if not isinstance(target, list):
        target = []
except (FileNotFoundError, json.JSONDecodeError):
    target = []
with open(source_path, "r", encoding="utf-8") as handle:
    source = json.load(handle)
target = [task for task in target if not str(task.get("label", "")).startswith("tc:")]
target.extend(source)
with open(target_path, "w", encoding="utf-8") as handle:
    json.dump(target, handle, indent=2)
    handle.write("\n")
PYTASKS

echo "==> Merged the five tc: tasks into ${ZED_CONFIG}/tasks.json"
echo
echo "Next steps:"
echo "  1. Open ${ROOT} in Zed (never sign in)."
echo "  2. Reload the dev extension: Zed -> Extensions -> Reinstall / Install Dev Extension -> ${ROOT}/editors/zed/tc-eng-extension"
echo "  3. If Zed was already open, run \"editor: restart language server\" from the command palette."
echo "  4. Open any .eng file for monochrome typographic highlighting, diagnostics, hover, completion, and go-to-definition."
echo "     Active code uses bold verbs/control, subtle-underlined variables, italic ACE anaphors and strings, and normal-weight numbers."
echo "     Explicit comments and COMMENT_OMITTED prose use muted italic gray."
