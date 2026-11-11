#!/usr/bin/env bash
# Regenerate spec/ from the upstream DEXPI Specification repository.
#
# The DEXPI domain models (Core, Plant, Process) are maintained as Python source
# and compiled to DEXPI XML by dexpi.specificator. The published model URIs under
# https://data.dexpi.org/ are namespace identifiers, not fetchable resources
# (see spec/VERSIONS.md), so we generate and vendor the model files ourselves.
#
# Requires: git, python >= 3.12
set -euo pipefail

TAG="${DEXPI_SPEC_TAG:-V2.0.0}"
REPO="https://gitlab.com/dexpi/Specification.git"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> cloning $REPO at $TAG"
git clone --depth 1 --branch "$TAG" "$REPO" "$WORK/spec"

echo "==> installing dexpi.specificator"
python3 -m venv "$WORK/venv"
"$WORK/venv/bin/pip" install --quiet --upgrade pip
"$WORK/venv/bin/pip" install --quiet "dexpi.specificator"

echo "==> generating model XML"
cat > "$WORK/spec/src/run_xml_only.py" <<'PY'
import pathlib, sys, logging
from dexpi.specificator.command_line import command_line
THIS_DIR = pathlib.Path(__file__).parent
logging.basicConfig(level=logging.INFO, format='%(message)s', stream=sys.stdout)
command_line(
    model_src=THIS_DIR / 'model',
    documentation_src=THIS_DIR / 'documentation',
    cache_dir=THIS_DIR / '.cache',
    args=['make', 'xml'])
PY
(cd "$WORK/spec/src" && "$WORK/venv/bin/python" run_xml_only.py)

echo "==> vendoring into $HERE/spec"
mkdir -p "$HERE/spec/models" "$HERE/spec/schema" "$HERE/spec/examples"
cp "$WORK"/spec/src/.build/xml/*.xml                              "$HERE/spec/models/"
cp "$WORK"/spec/src/documentation/_static/DEXPI_XML_Schema.xsd    "$HERE/spec/schema/"
cp "$WORK"/spec/src/documentation/_static/reference_pid.xml       "$HERE/spec/examples/"

echo "==> done. Upstream commit:"
git -C "$WORK/spec" log -1 --format='    %H %ad' --date=short
