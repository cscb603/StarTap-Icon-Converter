fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        // Compile resources.rc which includes the icon and version info
        embed_resource::compile("resources.rc", embed_resource::NONE);
    }
    println!("cargo:rerun-if-changed=resources.rc");
    // 只有当 icon.ico 存在时才记录变更
    if std::path::Path::new("icon.ico").exists() {
        println!("cargo:rerun-if-changed=icon.ico");
    }
}
