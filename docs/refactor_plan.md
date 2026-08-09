# Silent Refactor Plan

> 状态：已应用并完成一次去 AI-slop 复核（2026-08-08）。复核删除了机械拆分留下的
> package-wide 可见性、旧 façade、静默错误和 worker 默认值；实际文件边界见
> `docs/rust_architecture.md` 与 `docs/apple_integration.md`。

## 目标

这次重构只调整代码组织、依赖方向和测试归属，不同时增加产品功能。目标是让每个模块只有一个主要变化原因，同时继续保证 macOS、iPhone 和 CLI 共享同一套 Rust 产品行为。

当前最明显的集中点：

| 文件 | 当前规模 | 混合的主要职责 |
| --- | ---: | --- |
| `crates/app_ffi/src/lib.rs` | 约 6,100 行 | 应用服务、C ABI、DTO、package 迁移、播放会话、封面缓存、用户活动和大量测试 |
| `crates/store_sqlite/src/lib.rs` | 约 4,300 行 | schema、歌曲、歌单、队列、历史、封面、分析缓存、row mapping 和测试 |
| `apple/PlayerApp/Sources/PlayerShared/AppModel.swift` | 约 2,600 行 | 曲库、播放、歌单、编辑、worker、迁移、详情加载和展示缓存 |
| `apple/PlayerApp/Sources/PlayerShared/ContentView.swift` | 约 2,600 行 | macOS 根布局、曲库、播放中、队列、编辑表单、歌词和通用组件 |
| `apple/PlayerApp/Sources/PlayerShared/PhoneContentView.swift` | 约 2,800 行 | iPhone 导航、导入导出、播放中、歌单、详情、编辑、歌词和 UIKit bridge |
| `apple/PlayerApp/Tests/PlayerSharedTests/BuildOnly.swift` | 约 900 行 | 多个互不相关的 Swift 测试域 |

行数只是风险信号，不是拆分目标。最终判断标准是职责、依赖和测试边界是否清楚。

应用后的组合根规模：`app_ffi/src/lib.rs` 60 行、`AppModel.swift` 81 行、
`ContentView.swift` 140 行；`store_sqlite/src/lib.rs` 保留 schema 与共享映射，领域操作
已移至 5 个模块。iPhone 根状态 owner 保持集中（`PhoneRootView.swift` 约 1,160 行），
其独立页面、sheet、歌词组件和 UIKit bridge 已全部迁出。原 `BuildOnly.swift` 已按
10 个行为/支持文件拆分。

## 不变量

所有阶段都必须保持以下契约：

- SQLite schema、迁移行为和现有数据库兼容性不变。
- C ABI 导出符号、参数、JSON 字段和错误 envelope 不变。
- CLI 命令、输出格式和 `api_coverage` 契约不变。
- primary Music View 身份、导入去重、歌单引用和队列持久化语义不变。
- macOS/iPhone 的恢复状态、系统媒体控制、后台播放和中断处理不变。
- 不把 Rust 产品规则复制到 Swift 或 CLI。
- 每个阶段单独通过 CI；不保留 deprecated wrapper、旧 re-export 或双轨实现。

## 阶段 0：建立重构基线

本阶段已经开始完成：

- GitHub Actions 运行 Rust 格式、workspace tests、Clippy、Rust FFI 构建和 Swift tests。
- 修复当前 rustfmt 偏差，确保重构从全绿基线开始。

建议在开始结构调整前，再记录以下基线：

- C ABI 导出符号列表。
- 一份代表性 library package fixture 的 manifest 和导入结果。
- 当前 SQLite schema 快照。
- CLI `--help` 和 JSON contract fixtures。

这些快照只用于发现意外契约变化，不应复制实现逻辑。

## 阶段 1：拆分 `store_sqlite`

先处理最底层持久化边界，保持 `LibraryStore` 的公开 API 不变。建议目标结构：

```text
crates/store_sqlite/src/
  lib.rs                 # 模块声明、LibraryStore 和公开类型导出
  schema.rs              # schema 创建、版本检查和未来 migration 入口
  tracks.rs              # upsert、分页、查询、搜索、删除和 hash
  playlists.rs           # playlist CRUD、排序、最近使用和成员关系
  playback.rs            # 持久队列、收藏、历史和播放统计
  artwork.rs             # artwork bytes、asset 和 reference 表
  analysis.rs            # pending analysis、track/album loudness cache
  rows.rs                # SQLite row 到 domain model 的转换
  validation.rs          # 名称、rating、路径和查询 pattern 规范化
```

执行顺序：

1. 先移动无状态 helper 和 row mapping。
2. 再按数据域移动只读方法。
3. 最后移动带 transaction 的写操作。
4. 将当前巨型测试模块按相同数据域拆分；随机工作流 invariant test 保留为跨模块集成测试。

注意事项：

- transaction 必须仍由一个领域操作完整持有，不能为了拆文件把事务切开。
- schema SQL 保持单一来源。
- album key、playlist position 和 artwork resolution 等规则继续只实现一次。

## 阶段 2：拆分 `app_ffi`

先在现有 crate 内机械拆分，再决定是否新建 application-service crate。避免同时“搬文件 + 改架构”。建议目标结构：

```text
crates/app_ffi/src/
  lib.rs                 # crate 入口和稳定公开导出
  app.rs                 # PlayerApp 组合根与共享依赖
  dto/
    mod.rs
    library.rs
    playback.rs
    track.rs
    playlist.rs
    user.rs
  service/
    library.rs           # import/export/zero/audit/package
    tracks.rs            # details/edit/materialize/artwork/lyrics
    playlists.rs
    playback.rs          # queue、engine、lifecycle、snapshot
    user_activity.rs     # local profile 和播放 session history
  ffi/
    mod.rs               # response envelope、CString ownership、panic/error boundary
    library.rs
    tracks.rs
    playlists.rs
    playback.rs
    user.rs
  files/
    artwork_cache.rs
    managed_media.rs
    library_package.rs
```

约束：

- `ffi/*` 只能解析参数、调用 application method、序列化响应；不包含业务规则。
- `service/*` 不依赖 C string 或 JSON envelope。
- DTO 转换集中在 `dto/*`，不能散落于 C ABI wrapper。
- `PlayerApp` 仍是组合根；engine、store 和生命周期状态的所有权保持清楚。
- 现有 `#[no_mangle]` 名称逐个用符号快照核对。

当上述模块稳定后，再评估把 `service/*` 提取为新的 `crates/app_service`。只有在它能够完全不依赖 C ABI/JSON，而且 CLI 与 Apple 确实能直接共享其类型时才提取；否则留在 `app_ffi` 内部，避免只为目录美观增加 crate。

## 阶段 3：收紧 Rust 测试边界

- DTO、path validation、queue index mapping 等纯函数测试跟随所属模块。
- store 与 package round-trip 使用 crate integration tests。
- C ABI tests 只验证 ABI ownership、输入校验、JSON contract 和错误边界。
- 应用行为测试通过安全 Rust client 调用，避免多数测试都绕经 C string。
- 保留 CLI `api_coverage`，确保所有共享 application operation 都有 CLI contract。

阶段 1–3 完成后，先运行全部 Rust gate，再开始 Swift 调整。

## 阶段 4：拆分 `AppModel`

不要只按 extension 机械切文件，因为跨文件 extension 会迫使大量私有状态扩大可见性。按 feature ownership 提取状态 owner，由一个轻量组合根装配：

```text
PlayerShared/Model/
  AppModel.swift                  # 启动、依赖装配和跨 feature 协调
  AppFeatureState.swift           # library、playback、playlist、track detail、operation owner
  AppModel+Library.swift          # scope、分页、搜索、选择和 presentation cache 协调
  AppModel+Playback.swift         # snapshot、queue、seek、polling 和 lifecycle 协调
  AppModel+Playlists.swift        # recent/CRUD/settings/picker 协调
  AppModel+Tracks.swift           # details、rating、artwork、edit 和 materialize 协调
  AppModel+Analysis.swift         # import、analyze、audit、package 和 worker 协调
  PresentationPolicies.swift      # 时间、状态文案、排序和恢复等纯逻辑
```

迁移顺序：

1. 先提取无状态 policy 和格式化逻辑。
2. 提取 library maintenance，因为它与日常播放状态耦合最低。
3. 提取 track detail/edit。
4. 提取 playlist。
5. 最后提取 playback 和 library presentation；两者有当前选曲与队列的交叉，需要明确单向事件。

状态所有权规则：

- 一个 `@Published` 状态只能有一个 owner。
- 根模型只持有 5 个 feature state；调用方直接访问对应 owner，不保留旧扁平 façade。
- feature state 的 setter 限制在 `PlayerShared` target 内，外部 target 只能观察。
- 跨 feature 行为由 `AppModel+*.swift` 的内部协调方法完成，不扩大为 package-wide API。
- Rust snapshot 是播放事实来源；Swift 不建立第二套播放状态机。
- 迁移某个 feature 时，在同一变更中更新全部调用方，不保留旧 façade wrapper。

worker stdout 在边界处先校验为带关联值的 Swift enum；缺字段和未知事件是明确的协议
错误。UI 的穷举 switch 不使用事件名字符串、`?? 0` 或 `default: break`。

## 阶段 5：按平台和功能拆分 SwiftUI

macOS 和 iPhone 保持平台原生布局，不强行共享整页 View。共享数据格式、歌词时间线和小型无平台组件即可。

建议结构：

```text
PlayerShared/Views/
  Shared/
    Artwork/
    Lyrics/
    TrackRows/
    EmptyStates/
  Mac/
    MacRootView.swift
    MacSidebar.swift
    MacLibraryView.swift
    MacNowPlayingView.swift
    MacPlaybackBar.swift
    MacQueueSheet.swift
    MacTrackEditSheet.swift
    MacLibraryMaintenanceSheet.swift
  Phone/
    PhoneRootView.swift
    PhoneLibraryView.swift
    PhonePlaylistsView.swift
    PhoneNowPlayingView.swift
    PhoneQueueSheet.swift
    PhoneTrackDetailView.swift
    PhoneTrackEditSheet.swift
    PhoneFileBridges.swift
```

拆分顺序：

1. 先移动文件内已有的独立 `View`，不改布局。
2. 再拆根 View 的 sidebar/tab、library、Now Playing 和 sheets。
3. 最后识别 macOS/iPhone 真正重复的歌词、封面和行展示逻辑。
4. 每次只迁移一个可截图或可测试的界面区域。

避免创建一个带大量平台条件分支的“万能共享 View”；平台差异应留在 `Mac` 和 `Phone` 目录。

## 阶段 6：拆分 Swift 测试与完成文档

将 `BuildOnly.swift` 按行为域拆成：

```text
PlayerSharedTests/
  StartupTests.swift
  PresentationRestorationTests.swift
  LibraryMigrationTests.swift
  LibraryPresentationTests.swift
  PlaybackPolicyTests.swift
  PlaybackSystemIntegrationTests.swift
  LyricsTimelineTests.swift
  TrackIdentityTests.swift
  TestSupport.swift
```

测试文件名应描述行为，不使用 `BuildOnly` 这种实现阶段名称。共同 fixture/client 建立集中 `TestSupport`，但断言仍留在对应行为域。

同步更新 `docs/rust_architecture.md` 和 `docs/apple_integration.md`，只记录最终边界，不保留过渡结构。

## 每阶段验收门槛

Rust 阶段：

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Swift 阶段在 Rust gate 通过后执行：

```bash
cargo build -p app_ffi
swift test --package-path apple/PlayerApp
```

涉及 macOS UI 的阶段还必须打包、安装 `/Applications/Silent.app` 并验证系统只解析到这一份安装；涉及 iPhone UI 的阶段至少完成 simulator build，关键系统音频行为应在真机回归。

## 建议的合并单元

每个 PR/commit 只覆盖一个可验证边界，例如：

1. `store_sqlite` row mapping + tests。
2. `store_sqlite` playlist repository。
3. `store_sqlite` artwork repository。
4. `app_ffi` DTO 和 response boundary。
5. `app_ffi` package/artwork file services。
6. `app_ffi` playback service。
7. Swift library maintenance model。
8. Swift track detail model。
9. macOS 独立 sheets。
10. iPhone file bridges 和独立 sheets。

不要在同一变更里跨越 Rust store、FFI、AppModel 和两套 UI，除非是在迁移一个无法保持编译的窄接口，并且 Rust 测试已先完成。

## 完成定义

- 根 `lib.rs` 和根 View/Model 主要承担装配与导航，不再实现多个领域的细节。
- 文件名和目录能直接回答“这个行为应该改在哪里”。
- 生产文件通常控制在约 300–800 行；超过约 1,200 行时必须能说明它仍只有一个内聚职责。
- 没有为了拆文件扩大大量状态可见性。
- 没有 Swift/CLI 业务规则副本。
- 所有现有 CI gate、ABI/JSON/CLI contract、package round-trip 和安装验证持续通过。
