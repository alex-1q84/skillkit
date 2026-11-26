#!/usr/bin/env bash
# 隔离运行 skillkit 相关验证：整个子 shell 用临时 HOME，出口核对真实 ~/.skillkit 未被污染。
# 用法：make iso ARGS="skillkit remove unmanaged/demo --json"
# 测试本脚本的污染核对：SKILLKIT_ISO_REAL_HOME=<假 home> 时，快照与核对都指向该目录。
set -euo pipefail

[ $# -gt 0 ] || { echo "用法: make iso ARGS=\"<命令>\"（在临时 HOME 里执行，出口核对真实状态）" >&2; exit 2; }

TMP=$(mktemp -d /tmp/skillkit-iso.XXXXXX)
trap 'rm -rf "$TMP"' EXIT
REAL_HOME=${SKILLKIT_ISO_REAL_HOME:-$HOME}

REG="$REAL_HOME/.skillkit/registry.json"
POOL="$REAL_HOME/.skillkit/.agents/skills"
BRIDGE="$REAL_HOME/.agents/skills"

snapshot() {
  if [ -f "$1" ]; then shasum -a 256 "$1" | awk '{print $1}'; else echo "missing"; fi
}
snapshot_dir() {
  if [ -d "$1" ]; then ls "$1" | sort | shasum -a 256 | awk '{print $1}'; else echo "missing"; fi
}

before_reg=$(snapshot "$REG")
before_pool=$(snapshot_dir "$POOL")
before_bridge=$(snapshot_dir "$BRIDGE")

export HOME="$TMP"
"$@"

after_reg=$(snapshot "$REG")
after_pool=$(snapshot_dir "$POOL")
after_bridge=$(snapshot_dir "$BRIDGE")

if [ "$before_reg" != "$after_reg" ] || [ "$before_pool" != "$after_pool" ] || [ "$before_bridge" != "$after_bridge" ]; then
  echo "污染检测：隔离运行期间真实 skillkit 状态被改动！" >&2
  [ "$before_reg" != "$after_reg" ] && echo "  registry.json 变了：$REG" >&2
  [ "$before_pool" != "$after_pool" ] && echo "  canonical 池变了：$POOL" >&2
  [ "$before_bridge" != "$after_bridge" ] && echo "  ~/.agents/skills 变了：$BRIDGE" >&2
  echo "  隔离根目录保留在 $TMP，人工核查后删除" >&2
  trap - EXIT
  exit 1
fi

rm -rf "$TMP"
