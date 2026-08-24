fn main() {
    // tauri-build 默认不对图标文件声明 rerun-if-changed，换 ico 不会重嵌进 exe。
    // 这里显式声明，图标一改就触发构建脚本重跑（Windows exe/安装包/窗口/托盘图标）。
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/32x32.png");
    println!("cargo:rerun-if-changed=icons/64x64.png");
    println!("cargo:rerun-if-changed=icons/128x128.png");
    println!("cargo:rerun-if-changed=icons/128x128@2x.png");
    println!("cargo:rerun-if-changed=icons/icon.png");
    tauri_build::build()
}
