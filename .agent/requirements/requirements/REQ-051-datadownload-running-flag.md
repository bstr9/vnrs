---
id: REQ-051
title: "DataDownloadManager running 标志逻辑错误"
status: completed
created_at: "2026-04-20T16:00:00"
updated_at: "2026-04-20T19:25:00"
priority: P2
cluster: Bug-Fix
relations:
  depends_on: []
  related_to: [REQ-001]
versions:
  - version: 1
    date: "2026-04-20T16:00:00"
    author: ai
    context: "代码审查发现 data_download.rs:265 中 running 标志检查是恒真式且 running 从未被设为 true"
    reason: "初始发现"
    snapshot: "DataDownloadManager running 标志从未被设为 true，取消检查逻辑无效"
  - version: 2
    date: "2026-04-20T19:25:00"
    author: ai
    context: "修复完成：移除冗余条件，在下载开始/结束/出错时正确设置 running 标志，添加 cancel_download() 和 is_running() 方法"
    reason: "Bug 修复完成"
    snapshot: "DataDownloadManager running 标志逻辑正确，支持取消下载"
---

# DataDownloadManager running 标志逻辑错误

## 描述
`src/trader/data_download.rs:265` 中的条件 `!self.running.load(Ordering::SeqCst) && self.running.load(Ordering::SeqCst) == false` 是恒真式（`!x && x == false` 等价于 `!x`）。更重要的是，`running` 标志从未在代码中被设为 `true`，使得取消检查逻辑完全无效——下载数据时无法通过设置 `running = false` 来取消。

## 验收标准
- [x] 修复条件逻辑（移除冗余的 `&& self.running.load() == false`）
- [x] 在下载开始时设置 `running = true`
- [x] 在下载结束/出错时设置 `running = false`
- [x] 取消下载时设置 `running = false` 能实际中断下载循环
