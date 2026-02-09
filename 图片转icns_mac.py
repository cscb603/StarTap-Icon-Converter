import os
import sys
import shutil
import subprocess
import tkinter as tk
from tkinter import ttk, filedialog, messagebox
from PIL import Image, ImageDraw, ImageChops

# 修正导入逻辑：TkinterDnD.Tk 才是正确的基类
try:
    # 针对 Nuitka 打包环境修复 tkdnd 路径
    if getattr(sys, "frozen", False) or "__compiled__" in globals():
        base_dir = os.path.dirname(sys.executable)
        tkdnd_dir = os.path.join(base_dir, "tkinterdnd2", "tkdnd")
        if os.path.exists(tkdnd_dir):
            os.environ["TKDND_LIBRARY"] = tkdnd_dir

    from tkinterdnd2 import TkinterDnD, DND_FILES

    BaseClass = TkinterDnD.Tk
    HAS_DND = True
except Exception as e:
    print(f"拖放库加载失败详情: {e}")
    BaseClass = tk.Tk
    HAS_DND = False


class IcnsConverter(BaseClass):
    def __init__(self, file_path=None):
        super().__init__()

        self.title("星TAP · macOS 高清图标转换器")
        self.geometry("450x450")
        self.resizable(False, False)

        # 转换选项
        self.rounded_corner = tk.BooleanVar(value=True)

        # 适配 macOS 外观
        if sys.platform == "darwin":
            self.configure(bg="#f5f5f7")
            self.style = ttk.Style()
            self.style.theme_use("aqua")

        self._create_widgets()

        # 启用拖放
        if HAS_DND:
            try:
                self.drop_target_register(DND_FILES)
                self.dnd_bind("<<Drop>>", self._on_dnd_drop)
            except Exception as e:
                print(f"注册拖放失败: {e}")

        # 处理启动时传入的文件
        if file_path:
            self.after(500, lambda: self._process_file(file_path))

    def _create_widgets(self):
        main_frame = ttk.Frame(self, padding="20")
        main_frame.pack(fill=tk.BOTH, expand=True)

        header_label = tk.Label(
            main_frame,
            text="macOS .icns 专家生成器",
            font=("System", 18, "bold"),
            fg="#1d1d1f",
        )
        header_label.pack(pady=(0, 5))

        desc_label = tk.Label(
            main_frame,
            text="集成原版超采样抗锯齿算法\n生成符合 Apple 严格标准的高清图标",
            font=("System", 11),
            fg="#86868b",
            justify=tk.CENTER,
        )
        desc_label.pack(pady=(0, 15))

        set_frame = ttk.LabelFrame(main_frame, text=" 转换设置 ", padding=15)
        set_frame.pack(fill=tk.X, pady=5)

        ttk.Checkbutton(
            set_frame,
            text="应用 macOS 标准圆角 (4x 超采样抗锯齿)",
            variable=self.rounded_corner,
        ).pack(anchor=tk.W)

        # 拖放区提示
        self.drop_frame = tk.Frame(
            main_frame,
            height=120,
            bg="#ffffff",
            highlightthickness=2,
            highlightbackground="#d2d2d7",
            highlightcolor="#0066cc",
        )
        self.drop_frame.pack(fill=tk.X, pady=15)
        self.drop_frame.pack_propagate(False)

        if HAS_DND:
            dnd_text = "✨ 拖拽图片到这里 ✨\n(支持批量拖入)"
            dnd_color = "#0066cc"
        else:
            dnd_text = "❌ 拖放组件未加载\n请检查环境或点击下方按钮"
            dnd_color = "#ff3b30"

        self.drop_label = tk.Label(
            self.drop_frame,
            text=dnd_text,
            bg="#ffffff",
            fg=dnd_color,
            font=("System", 13, "bold"),
        )
        self.drop_label.pack(expand=True)

        self.select_btn = ttk.Button(
            main_frame, text="手动选择图片文件", command=self._on_select_file
        )
        self.select_btn.pack(ipady=8, fill=tk.X)

        self.status_label = tk.Label(
            main_frame, text="等待操作...", font=("System", 10), fg="#86868b"
        )
        self.status_label.pack(pady=(10, 0))

    def _on_dnd_drop(self, event):
        data = event.data
        import re

        files = re.findall(r"\{(.*?)\}|(\S+)", data)
        file_list = [f[0] if f[0] else f[1] for f in files]

        for file_path in file_list:
            if os.path.isfile(file_path):
                self._process_file(file_path)

    def _make_square(self, img):
        w, h = img.size
        size = max(w, h)
        new_img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        new_img.paste(img, ((size - w) // 2, (size - h) // 2))
        return new_img

    def _add_rounded_corners(self, img, radius_ratio=0.18):
        if img.mode != "RGBA":
            img = img.convert("RGBA")
        img = self._make_square(img)
        scale = 4
        w, h = img.size
        big_size = (w * scale, h * scale)
        mask = Image.new("L", big_size, 0)
        draw = ImageDraw.Draw(mask)
        radius = int(min(big_size) * radius_ratio)
        draw.rounded_rectangle(
            [0, 0, big_size[0] - 1, big_size[1] - 1], radius=radius, fill=255
        )
        mask = mask.resize((w, h), resample=Image.Resampling.LANCZOS)
        orig_alpha = img.getchannel("A")
        final_alpha = ImageChops.multiply(orig_alpha, mask)
        output = img.copy()
        output.putalpha(final_alpha)
        return output

    def _on_select_file(self):
        file_paths = filedialog.askopenfilenames(
            title="选择设计源文件",
            filetypes=[
                ("图像文件", "*.png;*.jpg;*.jpeg;*.webp;*.bmp"),
                ("所有文件", "*.*"),
            ],
        )
        for path in file_paths:
            self._process_file(path)

    def _process_file(self, source_path):
        try:
            self.status_label.config(
                text=f"正在处理: {os.path.basename(source_path)}", fg="#f5a623"
            )
            self.update()

            base_dir = os.path.dirname(source_path)
            file_name = os.path.splitext(os.path.basename(source_path))[0]
            iconset_dir = os.path.join(base_dir, f"{file_name}.iconset")
            output_icns = os.path.join(base_dir, f"{file_name}.icns")

            if os.path.exists(iconset_dir):
                shutil.rmtree(iconset_dir)
            os.makedirs(iconset_dir)

            with Image.open(source_path) as raw_img:
                if raw_img.mode != "RGBA":
                    raw_img = raw_img.convert("RGBA")
                processed_source = self._make_square(raw_img)

                sizes = [
                    ("icon_16x16.png", 16),
                    ("icon_16x16@2x.png", 32),
                    ("icon_32x32.png", 32),
                    ("icon_32x32@2x.png", 64),
                    ("icon_128x128.png", 128),
                    ("icon_128x128@2x.png", 256),
                    ("icon_256x256.png", 256),
                    ("icon_256x256@2x.png", 512),
                    ("icon_512x512.png", 512),
                    ("icon_512x512@2x.png", 1024),
                ]

                for name, target_size in sizes:
                    resized = processed_source.resize(
                        (target_size, target_size), resample=Image.Resampling.LANCZOS
                    )
                    if self.rounded_corner.get():
                        final_img = self._add_rounded_corners(resized)
                    else:
                        final_img = resized
                    final_img.save(os.path.join(iconset_dir, name), "PNG")

            result = subprocess.run(
                ["iconutil", "-c", "icns", iconset_dir, "-o", output_icns],
                capture_output=True,
                text=True,
            )
            shutil.rmtree(iconset_dir)

            if result.returncode == 0:
                self.status_label.config(
                    text=f"✅ 已生成: {os.path.basename(output_icns)}", fg="#28a745"
                )
            else:
                raise Exception(f"iconutil 失败: {result.stderr}")

        except Exception as e:
            self.status_label.config(text="转换失败", fg="#d73a49")
            messagebox.showerror(
                "错误", f"处理 {os.path.basename(source_path)} 出错：\n{str(e)}"
            )


if __name__ == "__main__":
    input_files = sys.argv[1:]
    app = IcnsConverter()
    if input_files:
        for f in input_files:
            if os.path.isfile(f):
                app.after(500, lambda path=f: app._process_file(path))
    app.mainloop()
