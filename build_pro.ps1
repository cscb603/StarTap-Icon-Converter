# 极致性能与最小体积编译脚本
Write-Host "🚀 开始执行星TAP 高清缩图极致优化编译..." -ForegroundColor Cyan

# 1. 检查 sccache
if (Get-Command sccache -ErrorAction SilentlyContinue) {
    Write-Host "✅ 检测到 sccache，已自动通过 .cargo/config.toml 启用" -ForegroundColor Green
} else {
    Write-Host "⚠️ 未检测到 sccache，将使用标准编译" -ForegroundColor Yellow
}

# 2. 清理旧产物 (可选，确保全新编译)
# cargo clean

# 3. 执行 release 编译
$startTime = Get-Date
cargo build --release

if ($LASTEXITCODE -eq 0) {
    $endTime = Get-Date
    $duration = $endTime - $startTime
    $exePath = "target\release\rust_image_compressor.exe"
    $size = (Get-Item $exePath).Length / 1MB
    
    Write-Host "`n==========================================" -ForegroundColor Cyan
    Write-Host "✨ 编译成功！" -ForegroundColor Green
    Write-Host "⏱️  耗时: $($duration.TotalSeconds.ToString("F2")) 秒"
    Write-Host "📦 产物体积: $($size.ToString("F2")) MB"
    Write-Host "🚀 路径: $exePath"
    Write-Host "==========================================" -ForegroundColor Cyan
    
    # 自动打开目录
    explorer.exe /select,$exePath
} else {
    Write-Host "`n❌ 编译失败，请检查错误日志" -ForegroundColor Red
}
