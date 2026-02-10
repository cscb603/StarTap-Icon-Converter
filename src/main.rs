#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::IconData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use image::{DynamicImage, GenericImageView, RgbaImage};
use fast_image_resize as fr;
use fast_image_resize::images::Image;
use jpeg_encoder::{Encoder, ColorType};
use img_parts::jpeg::Jpeg;
use img_parts::Bytes;
use rayon::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};
use anyhow::{Result, Context};
use std::fs;
use opener;
use std::collections::VecDeque;

// --- Image Processing Logic ---

#[derive(Debug, Clone, Copy, PartialEq)]
enum ProcessMode {
    WeChat, // 900KB, 2048px (Default)
    HD,     // 5MB, 4096px, sharpen
    Custom, // User defined
}

#[derive(Clone)]
struct ProcessConfig {
    mode: ProcessMode,
    target_kb: u32,
    max_dim: u32,
    quality: u8,
    output_dir: Option<PathBuf>,
    overwrite: bool,
    keep_original_name: bool,
}

impl ProcessConfig {
    fn validate(&mut self) {
        self.quality = self.quality.clamp(1, 100);
        self.max_dim = self.max_dim.clamp(1, 10000);
        if self.target_kb == 0 { self.target_kb = 10000; } // Default to high if user sets 0
    }
}

#[derive(Debug, Default)]
struct ImageFeatures {
    entropy: f32,
    is_graphic: bool,
    is_portrait: bool,
    is_landscape: bool,
}

struct Processor {
    config: ProcessConfig,
}

impl Processor {
    fn new(mut config: ProcessConfig) -> Self {
        config.validate();
        Self { config }
    }

    /// 核心处理函数：确保原子操作与异常恢复
    fn process_image(&self, input_path: &Path) -> Result<PathBuf> {
        let file_name = input_path.file_name().ok_or_else(|| anyhow::anyhow!("无效文件名"))?;
        
        // 1. 预检与加载
        let metadata = fs::metadata(input_path).context("读取文件元数据失败")?;
        if metadata.len() == 0 {
            return Err(anyhow::anyhow!("空文件跳过"));
        }

        let img = image::open(input_path)
            .with_context(|| format!("图片解码失败: {:?}", file_name))?;
        
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return Err(anyhow::anyhow!("图片尺寸无效"));
        }

        // 2. 智能分析与处理
        let features = self.analyze_content(&img);
        let (scale, new_width, new_height) = self.calculate_dimensions(width, height);
        
        let mut processed_img = self.resize_image(&img, width, height, new_width, new_height)?;
        
        if matches!(self.config.mode, ProcessMode::WeChat) {
            processed_img = self.apply_denoise(processed_img, &features);
        }
        processed_img = self.apply_sharpen(processed_img, &features, scale);

        // 3. 目标压缩与编码
        let rgb_img = processed_img.into_rgb8();
        let mut final_data = self.encode_with_target_size(&rgb_img, new_width, new_height)?;
        
        // 4. EXIF 信息迁移
        final_data = self.preserve_exif(input_path, final_data);

        // 5. 安全写入（原子性保证）
        let output_path = self.get_output_path(input_path)?;
        self.atomic_write(&output_path, &final_data)?;
        
        Ok(output_path)
    }

    fn calculate_dimensions(&self, w: u32, h: u32) -> (f32, u32, u32) {
        let current_max = w.max(h);
        if current_max > self.config.max_dim {
            let s = self.config.max_dim as f32 / current_max as f32;
            (s, (w as f32 * s) as u32, (h as f32 * s) as u32)
        } else {
            (1.0, w, h)
        }
    }

    fn resize_image(&self, img: &DynamicImage, w: u32, h: u32, nw: u32, nh: u32) -> Result<DynamicImage> {
        let rgba8 = img.to_rgba8();
        let src_image = Image::from_vec_u8(w, h, rgba8.into_raw(), fr::PixelType::U8x4)?;
        let mut dst_image = Image::new(nw, nh, fr::PixelType::U8x4);
        
        let mut resizer = fr::Resizer::new();
        resizer.resize(&src_image, &mut dst_image, &fr::ResizeOptions {
            algorithm: fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
            ..Default::default()
        })?;

        Ok(DynamicImage::ImageRgba8(
            RgbaImage::from_raw(nw, nh, dst_image.into_vec()).unwrap()
        ))
    }

    fn encode_with_target_size(&self, img: &image::RgbImage, w: u32, h: u32) -> Result<Vec<u8>> {
        let target_bytes = self.config.target_kb as usize * 1024;

        // 快速尝试默认质量
        let mut temp: Vec<u8> = Vec::new();
        Encoder::new(&mut temp, self.config.quality)
            .encode(img.as_raw(), w as u16, h as u16, ColorType::Rgb)?;

        if temp.len() <= target_bytes || self.config.target_kb >= 10000 {
            return Ok(temp);
        }

        // 二分法寻找最优质量 (40-95)
        let mut low = 40;
        let mut high = self.config.quality.min(95) as i32;
        let mut best_data = temp;

        while low <= high {
            let mid = (low + high) / 2;
            let mut current_temp: Vec<u8> = Vec::new();
            Encoder::new(&mut current_temp, mid as u8)
                .encode(img.as_raw(), w as u16, h as u16, ColorType::Rgb)?;

            if current_temp.len() <= target_bytes {
                best_data = current_temp;
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        Ok(best_data)
    }

    fn preserve_exif(&self, input_path: &Path, output_data: Vec<u8>) -> Vec<u8> {
        fs::read(input_path).ok().and_then(|input_bytes| {
            Jpeg::from_bytes(Bytes::from(input_bytes)).ok().and_then(|input_jpeg| {
                input_jpeg.segments().iter().find(|s| s.marker() == 0xE1).cloned().and_then(|exif_chunk| {
                    Jpeg::from_bytes(Bytes::from(output_data.clone())).ok().map(|mut output_jpeg| {
                        output_jpeg.segments_mut().insert(1, exif_chunk);
                        output_jpeg.encoder().bytes().to_vec()
                    })
                })
            })
        }).unwrap_or(output_data)
    }

    fn get_output_path(&self, input_path: &Path) -> Result<PathBuf> {
        if self.config.overwrite {
            return Ok(input_path.to_path_buf());
        }

        let file_stem = input_path.file_stem().unwrap().to_string_lossy();
        let suffix = if self.config.keep_original_name {
            ""
        } else {
            match self.config.mode {
                ProcessMode::HD => "_hdxiao",
                ProcessMode::WeChat => "_xiao",
                ProcessMode::Custom => "_opt",
            }
        };

        let output_dir = self.config.output_dir.clone()
            .unwrap_or_else(|| input_path.parent().unwrap().to_path_buf());

        if !output_dir.exists() {
            fs::create_dir_all(&output_dir)?;
        }

        let mut path = output_dir.join(format!("{}{}.jpg", file_stem, suffix));
        
        // 避免在非覆盖模式下且保存路径与原文件一致时发生冲突
        if !self.config.overwrite && path == input_path {
            path = output_dir.join(format!("{}_opt.jpg", file_stem));
        }

        Ok(path)
    }

    /// 原子性写入：先写临时文件再重命名，防止进程中断导致文件损坏
    fn atomic_write(&self, path: &Path, data: &[u8]) -> Result<()> {
        let temp_path = path.with_extension("tmp_writing");
        fs::write(&temp_path, data).context("写入临时文件失败")?;
        fs::rename(&temp_path, path).context("替换目标文件失败")?;
        Ok(())
    }

    fn analyze_content(&self, img: &DynamicImage) -> ImageFeatures {
        // Optimization: Analyze a downsampled version for speed
        let (w, h) = img.dimensions();
        let sampling_scale = if w.max(h) > 512 {
            512.0 / w.max(h) as f32
        } else {
            1.0
        };
        
        let sw = (w as f32 * sampling_scale) as u32;
        let sh = (h as f32 * sampling_scale) as u32;
        
        // Fast resize for analysis
        let thumb = img.resize_exact(sw, sh, image::imageops::FilterType::Nearest);
        let gray = thumb.to_luma8();
        let total_pixels = (sw * sh) as f32;
        
        let mut hist = [0u32; 256];
        let mut max_p = 0u8;
        let mut min_p = 255u8;
        
        for p in gray.pixels() {
            let v = p[0];
            hist[v as usize] += 1;
            if v > max_p { max_p = v; }
            if v < min_p { min_p = v; }
        }
        
        let mut entropy = 0.0f32;
        for &count in hist.iter() {
            if count > 0 {
                let prob = count as f32 / total_pixels;
                entropy -= prob * prob.log2();
            }
        }
        
        let contrast = (max_p - min_p) as f32 / 255.0;
        
        let is_graphic = entropy < 6.5 && contrast > 0.4;
        let is_portrait = entropy >= 6.5 && entropy <= 7.5 && contrast >= 0.2 && contrast <= 0.6;
        let is_landscape = entropy > 7.0 && contrast < 0.4;
        
        ImageFeatures {
            entropy,
            is_graphic,
            is_portrait,
            is_landscape,
        }
    }

    fn apply_denoise(&self, img: DynamicImage, features: &ImageFeatures) -> DynamicImage {
        if features.is_graphic {
            img
        } else if features.is_portrait {
            DynamicImage::ImageRgb8(image::imageops::blur(&img.to_rgb8(), 0.5))
        } else if features.is_landscape {
            DynamicImage::ImageRgb8(image::imageops::blur(&img.to_rgb8(), 0.5))
        } else {
            if features.entropy > 7.2 {
                DynamicImage::ImageRgb8(image::imageops::blur(&img.to_rgb8(), 0.3))
            } else {
                img
            }
        }
    }

    fn apply_sharpen(&self, img: DynamicImage, features: &ImageFeatures, scale: f32) -> DynamicImage {
        let radius = 1.0 * scale;
        let is_hd = matches!(self.config.mode, ProcessMode::HD);
        
        if is_hd {
            DynamicImage::ImageRgb8(image::imageops::unsharpen(&img.to_rgb8(), 1.8, 13))
        } else if features.is_graphic {
            DynamicImage::ImageRgb8(image::imageops::unsharpen(&img.to_rgb8(), (radius * 2.0).min(2.0), 15))
        } else if features.is_portrait {
            DynamicImage::ImageRgb8(image::imageops::unsharpen(&img.to_rgb8(), (radius * 1.5).min(1.2), 12))
        } else {
            let sharpness = (1.1 + (7.5 - features.entropy) * 0.1).clamp(1.0, 1.5);
            DynamicImage::ImageRgb8(image::imageops::unsharpen(&img.to_rgb8(), radius.min(1.5), (sharpness * 10.0) as i32))
        }
    }
}

// --- App Logic ---

enum AppMessage {
    TaskStarted(usize), // total files
    TaskProgress(usize, String), // completed count, current file name
    TaskFinished(Option<PathBuf>), // first output dir
}

struct CompressorApp {
    // State
    mode: ProcessMode,
    
    // Advanced Settings
    show_advanced: bool,
    custom_max_dim: u32,
    custom_quality: u8,
    custom_target_kb: u32,
    custom_output_dir: Option<PathBuf>,
    overwrite: bool,
    keep_original_name: bool,
    
    // Runtime
    is_processing: bool,
    show_overwrite_confirm: bool,
    pending_paths: Vec<PathBuf>,
    total_files: usize,
    completed_files: usize,
    current_file_name: String,
    progress: f32, // 0.0 to 1.0
    status_text: String,
    
    // Communication
    rx: Receiver<AppMessage>,
    tx: Sender<AppMessage>,
}

impl CompressorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Custom fonts
        let mut fonts = egui::FontDefinitions::default();
        
        // Robust font loading - Prioritize modern, "soft" UI fonts
        let font_paths = [
            "c:/windows/fonts/msyh.ttc",      // Microsoft YaHei (standard UI font)
            "c:/windows/fonts/msyhl.ttc",     // Microsoft YaHei Light (softer)
            "c:/windows/fonts/msyh.ttf",
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
        ];

        for path in font_paths {
            if let Ok(data) = fs::read(path) {
                let font_data = egui::FontData::from_owned(data);
                // Hinting can make edges look "harder", let's see if default is okay
                fonts.font_data.insert(
                    "custom_font".to_owned(),
                    font_data,
                );
                fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "custom_font".to_owned());
                fonts.families.entry(egui::FontFamily::Monospace).or_default().push("custom_font".to_owned());
                break;
            }
        }
        
        cc.egui_ctx.set_fonts(fonts);
        
        // Visuals: UI UX Pro Max Professional SaaS Style
        let mut visuals = egui::Visuals::light();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 250, 252); // Background
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59)); // Text
        visuals.widgets.noninteractive.rounding = 8.0.into();
        
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
        visuals.widgets.inactive.rounding = 8.0.into();
        
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(239, 246, 255);
        visuals.widgets.hovered.rounding = 8.0.into();
        
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.rounding = 8.0.into();

        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235); // Primary
        visuals.window_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.window_rounding = 12.0.into();
        
        cc.egui_ctx.set_visuals(visuals);

        // Load Icon (Embedded PNG for reliability)
        let icon_data = match image::load_from_memory(include_bytes!("../高速缩图图标.png")) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                Some(IconData {
                    rgba: rgba.into_raw(),
                    width: w,
                    height: h,
                })
            },
            Err(e) => {
                eprintln!("Failed to load icon: {}", e);
                None
            },
        };
        
        if let Some(icon) = icon_data.clone() {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(icon))));
        }

        let (tx, rx) = unbounded();

        Self {
            mode: ProcessMode::WeChat,
            show_advanced: false,
            custom_max_dim: 2048,
            custom_quality: 85,
            custom_target_kb: 900,
            custom_output_dir: None,
            overwrite: false,
            keep_original_name: false,
            
            is_processing: false,
            show_overwrite_confirm: false,
            pending_paths: Vec::new(),
            total_files: 0,
            completed_files: 0,
            current_file_name: String::new(),
            progress: 0.0,
            status_text: "✨ 准备就绪，欢迎使用星TAP 高清缩图".to_owned(),
            rx,
            tx,
        }
    }

    fn start_processing(&mut self, paths: Vec<PathBuf>, bypass_confirm: bool) {
        if paths.is_empty() { return; }

        if self.overwrite && !bypass_confirm {
            self.show_overwrite_confirm = true;
            self.pending_paths = paths;
            return;
        }

        self.is_processing = true;
        self.show_overwrite_confirm = false;
        self.progress = 0.0;
        self.status_text = "正在扫描文件...".to_owned();

        let tx = self.tx.clone();
        
        let config = match self.mode {
            ProcessMode::WeChat => ProcessConfig {
                mode: ProcessMode::WeChat,
                max_dim: 2048,
                quality: 85,
                target_kb: 900,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.overwrite,
                keep_original_name: self.keep_original_name,
            },
            ProcessMode::HD => ProcessConfig {
                mode: ProcessMode::HD,
                max_dim: 4096,
                quality: 95,
                target_kb: 5000,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.overwrite,
                keep_original_name: self.keep_original_name,
            },
            ProcessMode::Custom => ProcessConfig {
                mode: ProcessMode::Custom,
                max_dim: self.custom_max_dim,
                quality: self.custom_quality,
                target_kb: self.custom_target_kb,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.overwrite,
                keep_original_name: self.keep_original_name,
            }
        };
        
        std::thread::spawn(move || {
            let files = collect_files_recursive(paths);
            let total = files.len();
            
            if total == 0 {
                tx.send(AppMessage::TaskFinished(None)).unwrap();
                return;
            }

            tx.send(AppMessage::TaskStarted(total)).unwrap();
            
            let processor = Arc::new(Processor::new(config));
            let first_output_dir = Arc::new(std::sync::Mutex::new(None));
            let completed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

            files.par_iter().for_each(|path| {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                
                match processor.process_image(path) {
                    Ok(out_path) => {
                        let mut first_dir = first_output_dir.lock().unwrap();
                        if first_dir.is_none() {
                            *first_dir = Some(out_path.parent().unwrap().to_path_buf());
                        }
                    },
                    Err(e) => {
                        // 记录错误但不中断整体流程
                        eprintln!("跳过损坏图片 {:?}: {}", path.file_name(), e);
                    }
                }
                
                let current = completed_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                tx.send(AppMessage::TaskProgress(current, file_name)).unwrap();
            });

            let final_dir = first_output_dir.lock().unwrap().clone();
            tx.send(AppMessage::TaskFinished(final_dir)).unwrap();
        });
    }
}

fn collect_files_recursive(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = VecDeque::from(paths);
    let valid_exts = ["jpg", "jpeg", "png", "webp", "bmp"];

    while let Some(path) = queue.pop_front() {
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if valid_exts.contains(&ext.to_lowercase().as_str()) {
                    files.push(path);
                }
            }
        } else if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                let mut dir_entries: Vec<_> = entries.flatten().collect();
                // Sort entries for deterministic order
                dir_entries.sort_by_key(|e| e.path());
                for entry in dir_entries {
                    queue.push_back(entry.path());
                }
            }
        }
    }
    // Final sort of all collected files
    files.sort();
    files
}

fn load_icon() -> Option<IconData> {
    match image::load_from_memory(include_bytes!("../高速缩图图标.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            })
        },
        Err(_) => None,
    }
}

impl eframe::App for CompressorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle Messages
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::TaskStarted(total) => {
                    self.total_files = total;
                    self.completed_files = 0;
                    self.status_text = format!("🚀 正在准备处理 {} 张图片...", total);
                    self.progress = 0.0;
                    self.current_file_name = String::new();
                }
                AppMessage::TaskProgress(count, file_name) => {
                    self.completed_files = count;
                    self.current_file_name = file_name;
                    if self.total_files > 0 {
                        self.progress = count as f32 / self.total_files as f32;
                    }
                    self.status_text = format!("⚡ 正在处理 ( {} / {} )", count, self.total_files);
                }
                AppMessage::TaskFinished(output_dir) => {
                    self.is_processing = false;
                    self.progress = 1.0;
                    self.current_file_name = String::new();
                    self.status_text = "✨ 任务完成！所有图片已处理".to_owned();
                    if let Some(path) = output_dir {
                        let _ = opener::open(path);
                    }
                }
            }
        }

        // Drag & Drop
        if !self.is_processing && !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let dropped_paths: Vec<PathBuf> = ctx.input(|i| {
                i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
            });
            if !dropped_paths.is_empty() {
                self.start_processing(dropped_paths, false);
            }
        }

        // Header Panel (SaaS Style)
        egui::TopBottomPanel::top("header_panel")
            .frame(egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(40.0, 25.0))
                .fill(egui::Color32::from_rgb(255, 255, 255))) // Clear white background
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 280.0) / 2.0);
                        ui.label(egui::RichText::new("📸").size(36.0));
                        ui.add_space(15.0);
                        ui.label(egui::RichText::new("星TAP 高清缩图")
                            .size(30.0)
                            .strong()
                            .color(egui::Color32::from_rgb(30, 41, 59))); // Slate 800
                    });
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("企业级图片处理内核 · 智能压缩 · 极速出片")
                        .size(13.0)
                        .color(egui::Color32::from_rgb(100, 116, 139))); // Slate 500
                });
            });

        // Bottom Status Panel
        egui::TopBottomPanel::bottom("status_panel")
            .frame(egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(40.0, 20.0))
                .fill(egui::Color32::from_rgb(255, 255, 255)) // Consistency
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(241, 245, 249)))) // Subtle top border
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if self.is_processing {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&self.status_text).size(14.0).strong().color(egui::Color32::from_rgb(37, 99, 235)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{:.0}%", self.progress * 100.0)).size(14.0).strong().color(egui::Color32::from_rgb(30, 41, 59)));
                            });
                        });
                        ui.add_space(8.0);
                        let pb = egui::ProgressBar::new(self.progress)
                            .animate(true)
                            .rounding(4.0)
                            .fill(egui::Color32::from_rgb(37, 99, 235));
                        ui.add(pb);
                        ui.add_space(8.0);
                        if !self.current_file_name.is_empty() {
                            ui.add(egui::Label::new(
                                egui::RichText::new(format!("正在处理: {}", self.current_file_name))
                                    .size(11.0)
                                    .color(egui::Color32::from_rgb(100, 116, 139))
                            ).truncate(true));
                        }
                    } else {
                        ui.label(egui::RichText::new(&self.status_text).size(15.0).strong().color(egui::Color32::from_rgb(71, 85, 105)));
                    }
                    ui.add_space(15.0);
                    ui.label(egui::RichText::new("星TAP 实验室 | 高性能 Rust 内核 v2026").size(11.0).color(egui::Color32::from_rgb(148, 163, 184)));
                });
            });

        // Central Content Panel
        egui::CentralPanel::default().frame(
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(40.0, 20.0))
                .fill(egui::Color32::from_rgb(248, 250, 252)) // Slate 50 background
        ).show(ctx, |ui| {
            // Overwrite Confirmation Dialog
            if self.show_overwrite_confirm {
                egui::Window::new("⚠️ 确认覆盖原图")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .fixed_size([300.0, 150.0])
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new("您已开启“覆盖原图”选项。").color(egui::Color32::from_rgb(30, 41, 59)));
                            ui.label(egui::RichText::new("此操作不可撤销，确定要继续吗？").strong().color(egui::Color32::RED));
                            ui.add_space(20.0);
                            ui.horizontal(|ui| {
                                ui.add_space(40.0);
                                if ui.button(egui::RichText::new(" 取消 ").color(egui::Color32::from_rgb(71, 85, 105))).clicked() {
                                    self.show_overwrite_confirm = false;
                                    self.pending_paths.clear();
                                }
                                ui.add_space(20.0);
                                if ui.add(egui::Button::new(egui::RichText::new(" 确认覆盖 ").strong().color(egui::Color32::WHITE)).fill(egui::Color32::from_rgb(220, 38, 38))).clicked() {
                                    let paths = std::mem::take(&mut self.pending_paths);
                                    self.show_overwrite_confirm = false;
                                    self.start_processing(paths, true);
                                }
                            });
                        });
                    });
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Settings Group
                ui.add_space(10.0);
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .rounding(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)))
                    .inner_margin(egui::Margin::same(20.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("选择输出模式").strong().size(16.0).color(egui::Color32::from_rgb(30, 41, 59)));
                                
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.checkbox(&mut self.show_advanced, "高级设置");
                                });
                            });
                            ui.add_space(15.0);
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.mode, ProcessMode::WeChat, "微信优化 (900KB)");
                                ui.add_space(15.0);
                                ui.radio_value(&mut self.mode, ProcessMode::HD, "高清无损 (5MB)");
                                ui.add_space(15.0);
                                ui.radio_value(&mut self.mode, ProcessMode::Custom, "自定义参数");
                            });
                            
                            ui.add_space(15.0);
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut self.overwrite, "覆盖原图").on_hover_text("直接替换原始文件，请谨慎使用").changed() {
                                    if self.overwrite {
                                        self.keep_original_name = true;
                                    }
                                }
                                ui.add_space(20.0);
                                ui.add_enabled(!self.overwrite, egui::Checkbox::new(&mut self.keep_original_name, "保持原文件名"))
                                    .on_hover_text("输出时不添加后缀（若在原目录则自动加_opt）");
                            });
                        });

                        if self.mode == ProcessMode::Custom || self.show_advanced {
                            ui.add_space(20.0);
                            ui.separator();
                            ui.add_space(20.0);
                            
                            egui::Grid::new("adv_grid").num_columns(2).spacing([30.0, 15.0]).show(ui, |ui| {
                                ui.label(egui::RichText::new("长边限制 (px):").color(egui::Color32::from_rgb(71, 85, 105)));
                                ui.add(egui::DragValue::new(&mut self.custom_max_dim).clamp_range(100..=10000).speed(10.0).suffix(" px"));
                                ui.end_row();

                                ui.label(egui::RichText::new("压缩质量 (1-100):").color(egui::Color32::from_rgb(71, 85, 105)));
                                ui.add(egui::Slider::new(&mut self.custom_quality, 1..=100));
                                ui.end_row();

                                ui.label(egui::RichText::new("目标大小 (KB):").color(egui::Color32::from_rgb(71, 85, 105)));
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut self.custom_target_kb).clamp_range(0..=50000).speed(10.0).suffix(" KB"));
                                    ui.label(egui::RichText::new("(0 为不限制)").size(11.0).color(egui::Color32::GRAY));
                                });
                                ui.end_row();
                            });

                            ui.add_space(20.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("导出目录:").color(egui::Color32::from_rgb(71, 85, 105)));
                                let display_path = self.custom_output_dir.as_ref()
                                    .map(|p| p.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "默认 (原文件旁)".to_owned());
                                
                                ui.label(egui::RichText::new(display_path).color(egui::Color32::from_rgb(37, 99, 235)).strong());

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if self.custom_output_dir.is_some() {
                                        if ui.button("重置").clicked() { self.custom_output_dir = None; }
                                    }
                                    if ui.button("更改").clicked() {
                                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                            self.custom_output_dir = Some(path);
                                        }
                                    }
                                });
                            });
                        }
                    });

                ui.add_space(25.0);

                // Drop Zone (SaaS Style)
                let available_width = ui.available_width();
                let (rect, response) = ui.allocate_at_least(egui::vec2(available_width, 240.0), egui::Sense::click());
                
                let is_hovering = (ctx.input(|i| !i.raw.hovered_files.is_empty()) || response.hovered()) && !self.is_processing;
                
                let bg_color = if is_hovering { egui::Color32::from_rgb(239, 246, 255) } else { egui::Color32::WHITE };
                let stroke_color = if is_hovering { egui::Color32::from_rgb(37, 99, 235) } else { egui::Color32::from_rgb(226, 232, 240) };
                let stroke_width = if is_hovering { 2.5 } else { 1.5 };

                ui.painter().rect(rect, 16.0, bg_color, egui::Stroke::new(stroke_width, stroke_color));
                
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(45.0);
                            ui.label(egui::RichText::new("📥").size(48.0));
                            ui.add_space(15.0);
                            ui.label(egui::RichText::new("拖入图片或整个文件夹").size(22.0).strong().color(egui::Color32::from_rgb(30, 41, 59)));
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new("支持多选文件、嵌套目录自动扫描").size(13.0).color(egui::Color32::from_rgb(100, 116, 139)));
                            
                            ui.add_space(30.0);
                            ui.horizontal(|ui| {
                                ui.add_space((ui.available_width() - 280.0) / 2.0);
                                if ui.add_enabled(!self.is_processing, 
                                    egui::Button::new(egui::RichText::new("📁 选择图片").size(14.0).strong())
                                        .min_size(egui::vec2(130.0, 40.0))
                                        .rounding(8.0)
                                ).clicked() {
                                    if let Some(paths) = rfd::FileDialog::new()
                                        .add_filter("图片文件", &["jpg", "jpeg", "png", "webp", "bmp"])
                                        .pick_files() 
                                    {
                                        self.start_processing(paths, false);
                                    }
                                }
                                ui.add_space(20.0);
                                if ui.add_enabled(!self.is_processing, 
                                    egui::Button::new(egui::RichText::new("📂 选择目录").size(14.0).strong())
                                        .min_size(egui::vec2(130.0, 40.0))
                                        .rounding(8.0)
                                ).clicked() {
                                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                        self.start_processing(vec![path], false);
                                    }
                                }
                            });
                        });
                    });
                });

                if response.clicked() && !self.is_processing {
                    if let Some(paths) = rfd::FileDialog::new().pick_files() {
                        self.start_processing(paths, false);
                    }
                }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let icon = load_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([540.0, 700.0])
            .with_title("星TAP 高清缩图")
            .with_resizable(false)
            .with_icon(std::sync::Arc::new(icon.unwrap_or_default()))
            .with_drag_and_drop(true),
        ..Default::default()
    };
    
    eframe::run_native(
        "rust_image_compressor",
        options,
        Box::new(|cc| Box::new(CompressorApp::new(cc))),
    )
}
