# Rust 架构边界

依赖只允许从外层指向内层：

```text
player_ffi / CLI / workers
  -> SQLite, filesystem, metadata, analysis, Rodio adapters
  -> player_engine
  -> player_core
```

## `player_core`

核心只保存可确定测试的领域规则：

- track/view 领域模型；
- 播放队列及插入、移动、删除和清空规则；
- repeat、shuffle、seek 和当前曲目状态；
- 播放中断生命周期；
- loudness normalize 决策。

核心不包含目录扫描、数据库、音频设备、线程、C ABI 或基础设施错误。

## 外层 crate

- `player_library_fs` 负责目录扫描和 `std::fs::Metadata` 到领域 fingerprint 的转换。
- `player_error` 负责 I/O、audio、metadata、store、engine 和输入错误。
- `player_engine` 定义 `AudioBackend` port，串行执行命令并在 backend 完成后确认结果。
- `player_audio_rodio` 实现 backend。
- `player_store_sqlite` 实现本地持久化。
- `player_ffi` 是 Apple 产品的 composition root 和 C ABI。

workspace 内部 API 可以破坏性演进。调用方必须在同一变更中迁移；不增加 deprecated wrapper、旧 re-export、类型别名或双轨实现。
