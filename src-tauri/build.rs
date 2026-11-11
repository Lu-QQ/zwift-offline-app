fn main() {
    let mut attrs = tauri_build::Attributes::new();
    // 打包(release)时以管理员身份运行:改 hosts / 导入证书 / 绑定 80/443 都需要。
    // 开发(debug)时保持 asInvoker,避免每次 `tauri dev` 都弹 UAC。
    // ⚠ app.manifest 里禁止任何非 ASCII 字符(含中文注释):中文系统的 RC 工具链按
    // GBK 读取,UTF-8 中文会被打乱导致 SxS "无效的 Xml 语法",打包版启动即失败。
    if std::env::var("PROFILE").unwrap_or_default() == "release" {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        );
    }
    tauri_build::try_build(attrs).expect("failed to run tauri-build");
}
