import sys
import os
import tkinter as tk
from tkinter import ttk, filedialog
from PIL import Image, ImageDraw, ImageChops
import windnd  # 用于支持拖放功能


def get_resource_path(relative_path):
    """获取资源绝对路径，兼容脚本运行和 Nuitka 打包"""
    if hasattr(sys, "frozen") or hasattr(sys, "_MEIPASS"):
        # 如果是打包后的环境
        base_path = os.path.dirname(os.path.abspath(sys.argv[0]))
        # 对于 Nuitka onefile，资源通常在 __file__ 所在的临时目录
        bundle_path = os.path.dirname(os.path.abspath(__file__))
        path1 = os.path.join(bundle_path, relative_path)
        if os.path.exists(path1):
            return path1
    return os.path.join(os.path.dirname(os.path.abspath(__file__)), relative_path)


class IcoConverter(tk.Tk):
    def _set_icon(self):
        """设置窗口图标，支持打包后的路径"""
        if hasattr(sys, "_MEIPASS"):
            base_path = getattr(sys, "_MEIPASS")
        elif hasattr(sys, "frozen"):
            base_path = os.path.dirname(os.path.abspath(__file__))
        else:
            base_path = os.path.dirname(os.path.abspath(__file__))

        icon_path = os.path.join(base_path, "图片转ico图标.ico")
        if not os.path.exists(icon_path):
            icon_path = os.path.join(os.path.dirname(sys.argv[0]), "图片转ico图标.ico")

        if os.path.exists(icon_path):
            try:
                self.iconbitmap(icon_path)
            except Exception:
                pass

    def __init__(self, file_path=None):
        super().__init__()
        self.title("星TAP · 图片转高清ICO")
        self.geometry("380x300")
        self.resizable(False, False)
        self.option_add("*Font", ("Microsoft YaHei", 9))
        self._set_icon()

        self.rounded_corner = tk.BooleanVar(value=True)
        self.transparent_bg = tk.BooleanVar(value=True)

        self._create_widgets()
        windnd.hook_dropfiles(self, self._on_drag_drop)

        if file_path and os.path.isfile(file_path):
            self.after(100, lambda: self._process_file(file_path))

    def _on_drag_drop(self, files):
        """处理拖放的文件"""
        for file_path in files:
            if isinstance(file_path, bytes):
                file_path = file_path.decode("gbk")
            if os.path.isfile(file_path):
                self._process_file(file_path)

    def _create_widgets(self):
        """重构商业化精简界面"""
        # 主容器
        main_frame = ttk.Frame(self, padding="15 10")
        main_frame.pack(fill=tk.BOTH, expand=True)

        # 头部标题
        header_label = tk.Label(
            main_frame,
            text="星TAP 高清ICO转换器",
            font=("Microsoft YaHei", 14, "bold"),
            fg="#2c3e50",
        )
        header_label.pack(pady=(0, 10))

        # 核心设置区
        set_frame = ttk.LabelFrame(main_frame, text=" 转换预设 ", padding=10)
        set_frame.pack(fill=tk.X, pady=5)

        # 水平排列复选框
        check_inner = ttk.Frame(set_frame)
        check_inner.pack(expand=True)

        ttk.Checkbutton(
            check_inner,
            text="透明背景",
            variable=self.transparent_bg,
            state=tk.DISABLED,
        ).pack(side=tk.LEFT, padx=15)
        ttk.Checkbutton(
            check_inner, text="智能圆角", variable=self.rounded_corner
        ).pack(side=tk.LEFT, padx=15)

        # 拖放/操作区
        drop_frame = ttk.LabelFrame(main_frame, text=" 快速处理 ", padding=15)
        drop_frame.pack(fill=tk.BOTH, expand=True, pady=5)

        self.status_label = tk.Label(
            drop_frame,
            text="拖拽图片至此 或 点击下方按钮",
            fg="#7f8c8d",
            wraplength=300,
        )
        self.status_label.pack(expand=True, pady=(0, 10))

        btn_select = ttk.Button(
            drop_frame, text="选择图片文件", command=self._select_file, width=18
        )
        btn_select.pack()

        # 页脚：版权与联系方式
        footer_frame = ttk.Frame(main_frame)
        footer_frame.pack(fill=tk.X, pady=(10, 0))

        tk.Label(
            footer_frame,
            text="© 星TAP · cscb603@qq.com",
            font=("Consolas", 8),
            fg="#bdc3c7",
        ).pack(side=tk.RIGHT)

    def _select_file(self):
        """选择图片文件并处理"""
        file_path = filedialog.askopenfilename(
            title="选择图片文件",
            filetypes=[
                ("图片文件", "*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.webp"),
                ("所有文件", "*.*"),
            ],
        )
        if file_path:
            self._process_file(file_path)

    def _process_file(self, file_path):
        """处理图片转换的主流程"""
        if not os.path.exists(file_path):
            self._update_status("错误：文件不存在或无法访问", "red")
            return

        # 支持 WebP 扩展
        ext = os.path.splitext(file_path)[1].lower()
        if ext not in (".png", ".jpg", ".jpeg", ".bmp", ".gif", ".webp"):
            self._update_status("错误：不支持该文件格式", "red")
            return

        try:
            file_name = os.path.basename(file_path)
            self._update_status(f"正在处理：{file_name}", "#f57c00")

            ico_path = self._convert_to_ico(
                file_path, need_round=self.rounded_corner.get()
            )

            self._update_status(f"转换成功：{os.path.basename(ico_path)}", "blue")

        except Exception as e:
            self._update_status(f"转换失败：{str(e)}", "red")

    def _update_status(self, text, color):
        """更新状态显示"""
        self.status_label.configure(text=text, foreground=color)
        self.update_idletasks()

    def _make_square(self, img):
        """将图片放置在透明正方形画布中心，防止拉伸"""
        w, h = img.size
        size = max(w, h)
        new_img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        new_img.paste(img, ((size - w) // 2, (size - h) // 2))
        return new_img

    def _add_rounded_corners(self, img, radius_ratio=0.18):
        """高质量圆角裁剪：融合原始透明度并消除锯齿"""
        if img.mode != "RGBA":
            img = img.convert("RGBA")

        # 1. 先做成正方形
        img = self._make_square(img)

        # 2. 超采样绘制遮罩
        scale = 4
        w, h = img.size
        big_size = (w * scale, h * scale)

        # 创建透明度遮罩 (L 模式)
        mask = Image.new("L", big_size, 0)
        draw = ImageDraw.Draw(mask)

        radius = int(min(big_size) * radius_ratio)
        # 修正坐标：从 0 到 size-1
        draw.rounded_rectangle(
            [0, 0, big_size[0] - 1, big_size[1] - 1], radius=radius, fill=255
        )

        # 缩回到原始尺寸 (LANCZOS 抗锯齿)
        mask = mask.resize((w, h), resample=Image.Resampling.LANCZOS)

        # 3. 关键：融合透明度
        orig_alpha = img.getchannel("A")
        final_alpha = ImageChops.multiply(orig_alpha, mask)

        output = img.copy()
        output.putalpha(final_alpha)
        return output

    def _convert_to_ico(self, file_path, need_round=False):
        """高清转换图片为ICO，支持高质量缩放和PNG压缩"""
        with Image.open(file_path) as source_img:
            if source_img.mode != "RGBA":
                source_img = source_img.convert("RGBA")

            # 预处理
            if need_round:
                source_img = self._add_rounded_corners(source_img)
            else:
                # 即使不需要圆角，也应该做成正方形防止拉伸
                source_img = self._make_square(source_img)

            # 生成各个尺寸的高质量副本
            # ICO 建议尺寸：256, 128, 64, 48, 32, 24, 16
            target_sizes = [
                256,
                128,
                64,
                48,
                40,
                32,
                24,
                20,
                16,
            ]  # 增加了一些 Windows 常用的小尺寸
            icon_images = []

            for size in target_sizes:
                # 使用最高质量的 LANCZOS (即 Lanczos3) 算法
                resized_img = source_img.resize(
                    (size, size), resample=Image.Resampling.LANCZOS
                )
                icon_images.append(resized_img)

            ico_path = os.path.splitext(file_path)[0] + ".ico"

            # 保存 ICO
            icon_images[0].save(ico_path, format="ICO", append_images=icon_images[1:])

            return ico_path


if __name__ == "__main__":
    if len(sys.argv) > 1:
        # 如果有参数，直接循环处理
        app = IcoConverter()
        for arg in sys.argv[1:]:
            if os.path.isfile(arg):
                app._process_file(arg)
        # 3秒后自动关闭，如果没有其他操作
        app.after(3000, app.destroy)
        app.mainloop()
    else:
        app = IcoConverter()
        app.mainloop()
