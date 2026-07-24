# Apple 平台集成

## 架构

当前架构：

```text
SwiftUI app
  -> ApplePlayerController
      -> AVAudioSession / MediaPlayer
      -> MediaPlayer remote commands
      -> Files / security scoped resources
      -> Rust player_ffi C ABI
          -> player_engine
              -> Rodio / CoreAudio
          -> player_core
```

Rust workspace 负责：

- `player_core`：队列、播放状态、生命周期和 normalize 领域规则。
- `player_engine`：带命令完成确认的播放服务和 backend port。
- `player_audio_rodio`：Apple 当前使用的本地音频渲染 backend。
- `player_library_fs`：目录扫描与文件 fingerprint。
- `player_store_sqlite`：搜索、播放列表、收藏、历史和 artwork cache。
- metadata、fingerprint 与响度分析适配器。

Swift/Apple 层负责：

- SwiftUI 视图。
- AVAudioSession。
- AVAudioSession 激活和系统中断通知。
- 锁屏封面和进度。
- 耳机、CarPlay、AirPlay、系统音量。
- iOS 文件授权和沙盒访问。

Apple 平台细节保持原生，队列和播放规则保持跨平台一致。Swift 不拥有第二套播放状态机。

Normalize 增益由 Rodio 播放 backend 应用到渲染链路，不修改系统音量。`player_engine` 的命令只有在 backend 操作完成后才返回；Swift 随后读取已经确认的 snapshot，不使用固定延时猜测状态。

当前 iOS 外壳已经完成系统播放集成：使用 `.playback` / `.longFormAudio` 配置并按需激活 `AVAudioSession`，通过 `MPNowPlayingInfoCenter` 发布锁屏标题、艺人、专辑、封面、时长、进度和播放状态，通过 `MPRemoteCommandCenter` 接收播放、暂停、上一首、下一首、拖动进度、循环和随机命令。来电等系统中断与耳机断开事件会进入 Rust `PlaybackLifecycle` 状态机，只有中断前正在播放且系统允许恢复时才会自动恢复。模拟器 app bundle 声明了 `UIBackgroundModes = audio`。

资料库列表通过 Rust/SQLite 稳定排序的分页 API 加载；Swift 每完成一页就更新真实加载比例，避免大资料库启动时只显示无法判断进度的旋转指示器。Rust 服务初始化或调用失败会进入 `AppModel` 的可见错误状态，由 iPhone 界面持续显示并弹窗报告，不使用 `fatalError` 终止进程。

## macOS

macOS 版本建议使用：

- SwiftUI sidebar。
- `NSOpenPanel` 选择音乐目录。
- security-scoped bookmark 保存目录授权。
- 可选菜单栏 mini player。
- 可选 media key 支持。

## iPhone

iPhone 版本建议使用：

- SwiftUI tab navigation。
- Files picker 导入目录或文件。
- App 沙盒缓存曲库数据库。
- `AVAudioSessionCategoryPlayback` 支持后台播放。
- `MPNowPlayingInfoCenter` 显示锁屏信息。
- `MPRemoteCommandCenter` 接收播放/暂停/上一首/下一首。

## Rust 到 Swift

推荐 UniFFI：

- Rust 类型更容易暴露给 Swift。
- 生成绑定后 Swift 调用接近原生 API。
- 适合 `LibraryService`、`QueueService`、`NormalizeService` 这类接口。

也可以先用 C ABI：

- 上手更直接。
- ABI 稳定性可控。
- 但字符串、数组、错误处理会更繁琐。

## 后续工程结构

建议下一步增加：

```text
apple/
  PlayerApple.xcodeproj
  Shared/
    RustBridge.swift
    LibraryViewModel.swift
    PlayerViewModel.swift
  iOS/
  macOS/
```

Rust 构建产物：

```text
target/aarch64-apple-ios/release/libplayer_ffi.a
target/aarch64-apple-ios-sim/release/libplayer_ffi.a
target/release/libplayer_ffi.dylib
```

最终由 Xcode build phase 调用 cargo 构建并链接。
