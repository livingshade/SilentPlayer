# Silent

一个面向 macOS、iPhone 和通用 CLI 的本地音乐播放器。三个 target 共用 Rust 应用行为；当前已经支持曲库扫描、metadata 解析、SQLite 持久化、EBU R128 响度分析与缓存、播放状态服务，以及通过默认音频输出设备播放本地音频文件。

目标是做一个接近主流播放器体验的本地播放器：曲库扫描、播放队列、专辑/歌曲视图、搜索、收藏、后台播放、锁屏控制，以及最重要的响度归一化，避免不同歌曲之间音量忽大忽小。

## Music View 模型

播放器内部把每个可播放音乐条目建模成一个 `view`。每个 view 有稳定的 `view_id`，并指向自己的 `primary_view_id`；最初导入的托管音频副本就是 primary view。primary view 使用 `audio:<audio_hash>` 作为身份，因此导入时会按音频内容去重，而不是按文件名或 metadata 去重。

后续改名、剪裁时间、降低音质、换封面、转格式等都可以建模成从 primary view 派生出的新 view。当前代码已经持久化了 `view_id`、`primary_view_id`、`view_kind`、`transform_spec`、`quality_profile`、`format_name` 这些字段；其中音质、格式和变换 spec 先作为占位字段，后续导出/转码流水线会基于它们生成完全独立、可携带、可播放的音频文件。完整字段说明见 `docs/music_view_model.md`。

## 技术方向

- Rust 负责共享核心：曲库索引、播放队列、响度元数据、normalize 增益策略、持久化模型。
- Apple 端使用 SwiftUI 和系统媒体框架处理文件授权、音频会话、后台播放、锁屏控制、AirPlay 与耳机控制；音频渲染由 Rust `player_engine` + Rodio backend 负责。
- Rust 和 Swift 之间建议用 UniFFI 或 C ABI 连接。这样业务规则保持一套，Apple 平台能力走原生路径。

## 当前仓库内容

- `crates/player_core`: 无 I/O 依赖的 Rust 领域核心，包含队列、播放状态、生命周期和响度归一化规则。
- `crates/player_error`: 基础设施层共享的错误类型；核心领域错误不依赖它。
- `crates/player_library_fs`: 本地文件系统扫描和文件 fingerprint 适配器。
- `crates/player_analysis_ebur128`: Rust 响度分析后端，使用 Symphonia 解码并用 EBU R128/BS.1770 分析 track loudness，并可从已缓存的 track loudness 生成 album loudness。
- `crates/player_audio_rodio`: Rust CLI 播放后端，使用 rodio 打开默认音频输出设备并播放本地文件。
- `crates/player_engine`: 线程化播放服务层；命令在 backend 完成后才返回，并发布状态、曲目、gain、进度与错误事件。
- `crates/player_ffi`: macOS、iPhone 和 CLI 共用的 Rust 应用实现及 Apple C ABI 适配层，导入时会把音频复制到托管媒体库。
- `crates/player_analyzer`: 独立 loudness 分析 worker，后台分析并把结果持久化到 SQLite。
- `crates/player_metadata_lofty`: metadata 解析后端，读取 title、artist、album、duration 和内嵌 artwork。
- `crates/player_store_sqlite`: SQLite 曲库和缓存，保存 metadata、file fingerprint、loudness analysis、搜索索引字段、播放列表、收藏、播放历史和 artwork bytes。
- `crates/player_cli`: `silent` 通用 CLI target，覆盖共享曲库、Music View、播放列表、历史和播放控制能力。
- `test-assets/audio`: 真实下载的 Ogg Vorbis 音频 fixtures，用于解码、响度分析和播放 smoke tests。
- `docs/product_design.md`: 产品和界面设计。
- `docs/loudness_normalization.md`: 响度归一化设计。
- `docs/apple_integration.md`: macOS/iOS 集成方式。
- `docs/rust_architecture.md`: Rust crate 边界和依赖规则。

iOS 外壳当前还会配置系统播放音频会话、发布锁屏 Now Playing 信息、接收耳机/锁屏远程命令，并通过 Rust 生命周期策略处理系统中断和音频输出断开。模拟器打包脚本会为 app 声明后台音频模式。

## 运行

macOS SwiftUI app 的持久数据位置：

- 托管音频副本：`~/Music/NormalPlayer/Music`
- 曲库、收藏、播放历史和 loudness analyze cache：`~/Music/NormalPlayer/player_library.sqlite3`

UI 的 Import 会复制音频到托管目录，之后播放器只使用托管副本，不会改动导入来源目录。

当前机器没有安装 Rust 工具链时，需要先安装 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装后在仓库根目录运行测试并构建 CLI：

```bash
cargo test
cargo build -p silent_cli
target/debug/silent --version
target/debug/silent --cli --help
```

简单命令直接放在根层，产品功能通过明确的 CLI target 边界调用：

```bash
target/debug/silent --version
target/debug/silent --cli --db player_library.sqlite3 --media-root Music library import ~/Music
target/debug/silent --cli --db player_library.sqlite3 --media-root Music library list
target/debug/silent --cli --db player_library.sqlite3 --media-root Music library search "artist or title" --limit 25
target/debug/silent --cli --db player_library.sqlite3 --media-root Music library analyze
target/debug/silent --cli --db player_library.sqlite3 --media-root Music playback shell
```

完整命令、JSON 输出、Music View 编辑、曲库迁移及 target 覆盖矩阵见
[`docs/cli.md`](docs/cli.md)。

本机启动 macOS SwiftUI 调试版：

```bash
scripts/run_mac_swiftui.sh
```

打包 release 版 macOS app：

```bash
scripts/package_mac_app.sh
open dist/Silent.app
```

打包脚本会生成 `dist/Silent.app` 和 `dist/Silent-macos.zip`，并把 Rust FFI dylib、loudness analyzer worker、library worker 和 Silent 专用图标一起放进 app bundle。默认使用本机 ad-hoc 签名，适合本机运行；如果要分发到其他 Mac，还需要用 Developer ID 证书签名并 notarize。

仓库里的 `rust-toolchain.toml` 已声明 macOS 与 iOS 常用 target。第一次进入目录时，rustup 会按需安装 stable、rustfmt、clippy 和 Apple target。

CLI 的 `playback shell` 与 macOS/iPhone 使用相同的队列、normalize、历史会话和生命周期规则。单文件分析可以使用 `silent --cli track analyze <music-file>`。

## 测试

默认测试不要求音频输出设备：

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

真实播放 smoke tests 需要 macOS 上有可用默认音频输出设备，因此默认标记为 ignored：

```bash
cargo test -p player_audio_rodio --test playback_smoke -- --ignored --nocapture
cargo test -p silent_cli --test cli -- --ignored --nocapture
```

重新下载外部测试音频：

```bash
scripts/download_test_audio.sh
```

下载来源和授权记录在 `test-assets/audio/SOURCES.md`。

## Normalize 策略

默认推荐：

- 目标响度：`-16 LUFS`
- 峰值上限：`-1 dBTP`
- 播放模式：单曲 normalize
- 专辑连续播放时可切到 album normalize，保留专辑内部动态差异

`silent --cli library analyze` 先计算每首歌的 integrated LUFS 和 true peak，再按专辑分组，把已缓存的单曲响度按时长加权合成为 album loudness，并写回每首歌的 album gain 字段。

核心公式：

```text
gain_db = target_lufs - measured_lufs + user_preamp_db
gain_db = min(gain_db, peak_ceiling_dbtp - true_peak_dbtp)
linear_gain = 10 ^ (gain_db / 20)
```

更完整的说明见 `docs/loudness_normalization.md`。
