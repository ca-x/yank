//! Opt-in desktop integration. Never changes system startup during headless tests.
use anyhow::Result;
#[cfg(not(target_os = "android"))]
pub fn launch_at_login(enabled: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    anyhow::ensure!(exe.is_file(), "应用文件不存在");
    let auto = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Yank")
        .set_app_path(
            exe.to_str()
                .ok_or_else(|| anyhow::anyhow!("应用路径不是有效文字"))?,
        )
        .set_args(&["--background"])
        .build()?;
    if enabled {
        auto.enable()?;
    } else {
        auto.disable()?;
    }
    Ok(())
}
#[cfg(target_os = "android")]
pub fn launch_at_login(_: bool) -> Result<()> {
    anyhow::bail!("安卓请使用系统应用启动管理")
}
#[cfg(not(target_os = "android"))]
pub fn update_unavailable_reason() -> Option<&'static str> {
    crate::updater::unavailable_reason()
}
#[cfg(target_os = "android")]
pub fn update_unavailable_reason() -> Option<&'static str> {
    Some("安卓请前往 GitHub 发布页手动下载安装更新；应用内不会自动替换 APK。")
}
#[cfg(not(target_os = "android"))]
pub fn update() -> Result<String> {
    crate::updater::update()
}
#[cfg(target_os = "android")]
pub fn update() -> Result<String> {
    anyhow::bail!("安卓请从发布页安装更新；测试签名 APK 可能需要先备份再重装")
}
