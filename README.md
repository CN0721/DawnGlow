# 朝霞预报 DawnGlow

基于 [Open-Meteo](https://open-meteo.com)（NCEP **GFS** / ECMWF **IFS** / **CAMS**）的
日出日落**火烧云（朝霞 / 晚霞）预报**安卓应用，纯 **Rust** + `egui`/`glow` 编写，
无 Java / Kotlin 业务代码（仅桌面小组件用少量 Java），单文件原生 Activity。

> 本仓库遵循 **AGPL-3.0** 开源，是 Folia (Rust) 的衍生作品，详见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)。

## 功能

- **地点**：当前定位（JNI 走 `LocationManager.getLastKnownLocation`）或地图选点。
- **事件预报**：逐日列出朝霞 / 晚霞时间与**鲜艳度评分**（0–100），一键切换朝 / 晚霞。
- **评分模型**：移植 r-ayin/sunset-prediction 的 v2.0 五因子模型（云型配置≈35%、能见度≈25%、
  湿度 15%、降水 10%、总云量修正 15%），并叠加 **CAMS 气溶胶光学厚度（AOD）** 因子。
- **要素详情**：高 / 中 / 低云量条、能见度、湿度、降水、评分因子逐项贡献。
- **多模式**：GFS / ECMWF / 融合（Blend）可切换；预报与空气质量**并行**请求，带重试与看门狗超时。
- **地图**：CARTO 深 / 浅色、OSM、高德（GCJ-02 纠偏）多图源；双指缩放 / 平移、惯性、瓦片磁盘缓存。
- **桌面小组件**：遥测摘要（时间、鲜艳度、云量、气溶胶）由 Rust 经 JNI 供 Java 绘制。
- **字体**：不内置字体，可选择系统字体、填入字体文件路径，或从**官方 / 开源分发**链接
  在线下载（支持 zip 整包，流式解压，带环形进度）；另有自定义直链入口。
- **外观**：统一视觉标尺（字号 / 间距 / 圆角 / 材质令牌）、骨架屏 + 斜向扫光 + 错峰浮现、
  朝晚霞非线性切换动画、滑动开关。

## 构建

需要 Android NDK 与 Rust `aarch64-linux-android` 目标。仓库内含 A 容器（AiCode，Alpine aarch64 +
PRoot）的交叉编译配置，见 [`.cargo/config.toml`](.cargo/config.toml) 与 [`scripts/build_android.sh`](scripts/build_android.sh)。

```sh
export PATH="$HOME/.cargo/bin:$PATH"
bash scripts/build_android.sh          # 产出 build/DawnGlow-0.1.0-debug.apk
```

桌面预览（可选）：

```sh
cargo run --bin dawnglow-desktop
```

## 数据与署名

| 来源 | 用途 | 授权 |
|---|---|---|
| Open-Meteo（GFS / ECMWF / CAMS） | 气象与空气质量数据 | CC BY 4.0，需署名 |
| © OpenStreetMap contributors | OSM 瓦片 | ODbL |
| © CARTO | CARTO 底图 | 见 CARTO 条款 |
| 高德地图（非官方公开瓦片） | 中文底图 | 仅供个人学习，版权归高德 |

应用内「关于」页与地图角落均已显示对应署名。

## 免责声明

本项目仅供**个人学习与技术研究**，不提供任何担保。气象预报存在不确定性，请勿用于
安全攸关的决策。高德图源为非官方接口，可能随时失效或涉及合规风险，请勿用于生产环境。
本项目**不内置任何字体**，字体页的下载链接均指向各厂商官方分发或开源字体，字体版权归
各权利方所有，使用时请遵守其授权协议。
