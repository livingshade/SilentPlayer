# Rust 架构边界

依赖只允许从外层指向内层：

```text
macOS adapter / iPhone adapter / silent CLI / workers
  -> shared PlayerApp application behavior
  -> SQLite, filesystem, metadata, analysis, Rodio adapters
  -> engine
  -> domain
```

## `domain`

核心只保存可确定测试的领域规则：

- 单一 primary 歌曲身份与 track 领域模型；
- 带稳定内部 ID 的全局循环队列，以及插入、移动、删除和清空规则；
- 单曲循环、顺序播放、随机播放三种互斥模式，及 seek 和当前曲目状态；
- 跨多个完整周期物化、可精确恢复的 shuffle 路径；
- 播放中断生命周期；
- loudness normalize 决策。

核心不包含目录扫描、数据库、音频设备、线程、C ABI 或基础设施错误。

## 外层 crate

- `library_fs` 负责目录扫描和 `std::fs::Metadata` 到领域 fingerprint 的转换。
- `errors` 负责 I/O、audio、metadata、store、engine 和输入错误。
- `engine` 定义 `AudioBackend` port，串行执行命令并在 backend 完成后确认结果。
- `audio_rodio` 实现 backend。
- `store_sqlite` 实现本地持久化。`lib.rs` 只保留公开类型、schema/连接生命周期和
  共享 row helper；歌曲、歌单、播放历史、metadata/artwork 与分析缓存分别位于
  `tracks.rs`、`playlists.rs`、`playback.rs`、`metadata_artwork.rs` 和 `analysis.rs`。
- `playback_store_sqlite` 单独保存全局队列、内部 ID、当前位置和已经物化的 shuffle
  路径；它不属于 Library 数据库，也不会进入曲库导出包。
- `app_ffi` 承载共享 `PlayerApp` composition root，并提供两个薄入口：
  Apple target 使用 C ABI，`silent` CLI target 使用安全 Rust client。两者调用同一个
  托管导入、歌曲原地编辑与独立导出、曲库迁移、播放列表、用户活动和播放会话实现。
  `service_*` 模块按曲库、歌曲、歌单和播放行为组织安全 Rust application methods；
  `ffi.rs` 只暴露稳定 C ABI；DTO、FFI ownership/error boundary、文件操作与运行时状态
  分别由独立模块维护。`lib.rs` 只负责模块装配和 `PlayerApp` 状态所有权；不把
  `ffi::*` 重导出成 Rust API。生产模块使用显式依赖，`use super::*` 只允许留在测试根。
- `silent_cli` 生成公开的 `silent` executable。根层只处理 `--version`/`--help`，
  共享产品命令必须经过 `silent --cli`。
- `analyzer` 与 `library_worker` 是 Apple app 的内部进度 worker，不是公开
  CLI target。

workspace 内部 API 可以破坏性演进。调用方必须在同一变更中迁移；不增加 deprecated wrapper、旧 re-export、类型别名或双轨实现。

application/store 查询的 `limit == 0` 是无效输入，不自动改成 1。队列恢复、播放会话历史、
文件 metadata 和 sidecar 目录读取失败会被传播或写入可见的 nonfatal error；只在关闭阶段
允许记录错误后继续释放资源。

## 测试边界

- `store_sqlite/src/tests/` 按 schema、歌曲、歌单、播放、封面和分析域组织。
- `app_ffi/src/tests/` 按曲库 package、歌曲、歌单、播放和用户活动组织。
- C ABI 导出集中在 `app_ffi/src/ffi.rs`，重构时用导出符号快照核对兼容性。
- CLI `api_coverage` 递归检查 `app_ffi/src`，确保拆分文件后共享 application operation
  仍有对应 CLI contract。
