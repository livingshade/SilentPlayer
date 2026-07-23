# 响度归一化设计

## 为什么不是简单 peak normalize

Peak normalize 只看波形最高点，会把峰值相同但听感差异很大的歌曲处理成同样增益。真实听感更接近平均响度、频率权重、动态范围和峰值余量，所以播放器应使用 EBU R128 / ITU-R BS.1770 风格的响度分析。

## 推荐数据

每首歌保存：

- `integrated_lufs`: 整首歌综合响度。
- `true_peak_dbtp`: true peak，防止播放时削波。
- `album_integrated_lufs`: 同专辑响度，可选。
- `album_true_peak_dbtp`: 同专辑峰值，可选。
- `analysis_version`: 分析算法版本。
- `analyzed_at`: 分析时间。

## 播放时增益

单曲模式：

```text
source_lufs = track.integrated_lufs
gain_db = target_lufs - source_lufs + user_preamp_db
```

专辑模式：

```text
source_lufs = track.album_integrated_lufs
gain_db = target_lufs - source_lufs + user_preamp_db
```

防削波：

```text
max_gain_without_clipping = peak_ceiling_dbtp - true_peak_dbtp
gain_db = min(gain_db, max_gain_without_clipping)
```

线性增益：

```text
linear_gain = 10 ^ (gain_db / 20)
```

这个增益应该应用到单首曲目的音频流上，而不是修改系统音量。用户的系统音量仍然代表“整体听歌音量”，normalize 只负责不同歌曲之间的相对一致性。

## 默认值

建议默认：

- 标准目标：`-16 LUFS`
- 响亮目标：`-14 LUFS`
- 安静目标：`-18 LUFS`
- true peak ceiling：`-1 dBTP`
- max boost：`+12 dB`
- 防削波：开启

## 分析时机

推荐流程：

1. 导入音乐后先显示在曲库里。
2. 后台低优先级分析响度。
3. 分析完成后更新曲目 normalize 状态。
4. 曲目分析完成后按 album/album artist 分组生成 album loudness。
5. 正在播放的曲目如果刚分析完成，下一首开始应用新增益，避免当前歌曲突然变音量。

## Rust 实现建议

Rust 核心应分为两层：

- `loudness policy`: 根据已经测得的 LUFS/peak 算播放增益，这部分必须纯 Rust、可单元测试、可跨平台共享。
- `loudness analyzer`: 解码音频并计算 LUFS/true peak。当前 Rust CLI 使用 `symphonia` 解码，再通过 `ebur128-stream` 计算 EBU R128 integrated LUFS 和 BS.1770 true peak。

iOS 上后台分析要注意：

- 大文件分析会耗电，应在充电/Wi-Fi/用户空闲时优先跑。
- 可以限制并发，默认 1 个分析任务。
- 分析结果需要持久化，避免重复扫。

## Metadata 优先级

读取文件时建议按以下顺序：

1. 文件里已有的 ReplayGain / R128 tag。
2. 本 app 之前分析并缓存的结果。
3. 后台重新分析。
4. 没有结果时临时使用 `0 dB` 增益，并标记为待分析。

Silent CLI 已支持：

```bash
silent --cli track analyze <music-file>
silent --cli --db player_library.sqlite3 --media-root Music library import <music-folder>
silent --cli --db player_library.sqlite3 --media-root Music library analyze
silent --cli --db player_library.sqlite3 --media-root Music playback shell
```

`track analyze` 会分析文件并输出 integrated LUFS 与 true peak。`playback shell` 播放曲库中的 Music View 时会使用与 macOS/iPhone 相同的 `NormalizationSettings`。

`library analyze` 会从 SQLite 曲库中找出缺少分析结果、分析版本过期、或文件 fingerprint 已变化的曲目，批量分析后写回缓存。

同一次 `library analyze` 会读取已缓存的 track loudness 和 duration，按专辑时长加权合成 `album_integrated_lufs`，并取同专辑内最大的 `true_peak_dbtp` 作为 `album_true_peak_dbtp`。如果某张专辑还有曲目缺少 track analysis 或 duration，就先跳过，等后续分析补齐后再处理。
