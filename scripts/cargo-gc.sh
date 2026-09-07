#!/usr/bin/env bash
# 清理 workspace 根 target/ 的陈旧编译产物。
# Cargo 从不回收旧 hash 产物，tauri dev 每次热重编都会留副本，重度开发一个月可积 39GB。
# 策略：删除 mtime 超过 N 天（默认 7）的文件（等价 cargo-sweep --time），
# 保留近期产物以维持增量构建缓存。硬链接的根级文件（libapp_lib.a 等）与 deps 副本共享 inode/mtime，会被一致处理。
# 用法：scripts/cargo-gc.sh [天数]
set -euo pipefail
cd "$(dirname "$0")/.."
DAYS="${1:-7}"

# 正在编译/dev 时跳过，避免与 rustc 抢文件
if pgrep -f "tauri dev|cargo (build|check|run|test)" >/dev/null 2>&1; then
  echo "检测到正在编译或 dev 运行，本次跳过" >&2
  exit 1
fi

if [ ! -d target ]; then
  echo "target/ 不存在，无需清理"
  exit 0
fi

before=$(du -sh target | cut -f1)
find target -type f -mtime +"$DAYS" -delete 2>/dev/null || true
find target -mindepth 1 -type d -empty -delete 2>/dev/null || true
after=$(du -sh target 2>/dev/null | cut -f1 || echo "0B")

echo "target/: ${before} -> ${after}（保留近 ${DAYS} 天产物）"
