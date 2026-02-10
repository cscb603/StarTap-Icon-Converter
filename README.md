# 星TAP 高速缩图 | StarTAP Image Shrinking Tool (Rust v3.0 High-Performance Edition)

## 🚀 2026 全新 Rust 引擎升级版

**StarTap Image Shrinking Tool** 现已全面升级至 Rust 引擎！专为微信、朋友圈及网络发图打造的“宝藏级”缩图工具。在保留原有智能逻辑的基础上，利用 Rust 原生性能实现了秒级的处理速度，让您的图片在网络分享时真正实现“小而美”。

---

### 🇨🇳 中文介绍

精准攻克图片在微信发送、朋友圈发布时被二次压缩的难题。Rust 版本通过 **SIMD 指令集加速**和**多线程并行处理**，让即便上千张的大图处理也能稳如泰山。它智能判断 2048px 与 900KB 的阈值，通过渐进式压缩算法，确保“体积小”与“清晰度高”完美兼得，彻底告别微信模糊图。

### 🇺🇸 English Introduction

**StarTap Image Shrinking Tool** has been fully re-engineered with Rust! Specifically tailored for WeChat and social media sharing. The Rust version leverages **SIMD acceleration** and **multi-threaded parallel processing**, delivering lightning-fast speeds even for thousands of high-res images. It intelligently balances the 2048px/900KB threshold using progressive compression, ensuring your photos stay "small in size" but "high in definition"—truly achieving "small and beautiful" without the worries of platform re-compression.

---

### 🛠️ 核心功能亮点 (Rust v3.0)

1.  **极速并行处理**：基于 Rayon 并行框架，自动榨干 CPU 每一核性能，多图处理速度提升 5-10 倍。
2.  **智能尺寸适配**：严格遵循 2048px 黄金准则，规避平台二次压缩。支持自定义 3:4 智能裁剪。
3.  **双重优化模式**：
    *   **微信优化模式**：精准控制在 900KB 以内，社交分享首选。
    *   **无损高清模式**：保留最高画质细节，适合专业摄影展示。
4.  **新增进阶控制**：
    *   **覆盖原图**：支持直接替换原始文件（带安全确认弹窗）。
    *   **保持原名**：导出到其他文件夹时可选择不增加后缀，保持干净的文件名。
5.  **工业级稳定性**：自动跳过损坏图片，支持 1000+ 数量级的批量任务不卡死、不崩溃。
6.  **锐度强化 (LANCZOS)**：采用最高等级的重采样算法，缩小后的图片比原图更锐利。

---

### 💻 技术栈 (Tech Stack)

*   **Language**: Rust (2024/2026 Edition)
*   **GUI**: egui / eframe (原生硬件加速界面)
*   **Processing**: Image crate + fast_image_resize (SIMD 加速)
*   **Parallelism**: Rayon (多线程)
*   **Compiler Optimization**: sccache + LTO + Profile-Guided Optimization

--- 

### 🚀 使用指南

#### 【Windows 系统】
1.  **解压即用**：下载 `rust_image_compressor.exe`。
2.  **拖拽处理**：将图片或整个文件夹直接拖入窗口。
3.  **灵活配置**：在界面左侧勾选“覆盖原图”或“保持原名”，点击开始即可。
4.  **查看结果**：默认在原图旁生成 `_opt` 后缀文件，或直接覆盖。处理完会自动弹出结果目录。

#### 【开发者/编译步骤】
如果您想从源代码构建以获得极致性能：
```powershell
# 确保已安装 Rust 环境
cargo build --release
```
*注：本项目已配置 `.cargo/config.toml` 自动启用 `sccache` 加速编译。*

---

### 📅 更新日志

*   **v3.0 (2026/01/09)**：
    *   由 Python 迁移至 Rust，处理性能质的飞跃。
    *   新增“覆盖原图”及“保持原名”功能选项。
    *   引入智能下采样分析，识别大图内容速度提升 50 倍。
    *   修复了旧版处理损坏图片时可能卡死的问题。
*   **v2.6 (2025/10/23)**：
    *   优化了微信 900KB 阈值的智能判断逻辑。

---

### 📮 反馈与支持

在使用中有任何建议或遇到 Bug，欢迎反馈：
📧 邮箱：**cscb603@qq.com**

> **程序说明**：缩图大小为了适配朋友圈和网络分享，又小又清楚。2048px 与 900KB 是微信压缩的生死线，交给 StarTap，您只管分享美好！