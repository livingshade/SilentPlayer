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

- track/view 领域模型；
- 播放队列及插入、移动、删除和清空规则；
- repeat、shuffle、seek 和当前曲目状态；
- 播放中断生命周期；
- loudness normalize 决策。

核心不包含目录扫描、数据库、音频设备、线程、C ABI 或基础设施错误。

## 外层 crate

- `library_fs` 负责目录扫描和 `std::fs::Metadata` 到领域 fingerprint 的转换。
- `errors` 负责 I/O、audio、metadata、store、engine 和输入错误。
- `engine` 定义 `AudioBackend` port，串行执行命令并在 backend 完成后确认结果。
- `audio_rodio` 实现 backend。
- `store_sqlite` 实现本地持久化。
- `app_ffi` 当前承载共享 `PlayerApp` composition root，并提供两个薄入口：
  Apple target 使用 C ABI，`silent` CLI target 使用安全 Rust client。两者调用同一个
  托管导入、Music View、曲库迁移、播放列表、用户活动和播放会话实现。
- `silent_cli` 生成公开的 `silent` executable。根层只处理 `--version`/`--help`，
  共享产品命令必须经过 `silent --cli`。
- `analyzer` 与 `library_worker` 是 Apple app 的内部进度 worker，不是公开
  CLI target。

workspace 内部 API 可以破坏性演进。调用方必须在同一变更中迁移；不增加 deprecated wrapper、旧 re-export、类型别名或双轨实现。
