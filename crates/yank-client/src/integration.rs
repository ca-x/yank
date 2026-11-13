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
pub fn update() -> Result<String> {
    static UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    use std::sync::atomic::Ordering;
    if UPDATING.swap(true, Ordering::AcqRel) {
        return Ok("更新正在进行".into());
    }
    struct Unlock;
    impl Drop for Unlock {
        fn drop(&mut self) {
            UPDATING.store(false, Ordering::Release);
        }
    }
    let _unlock = Unlock;
    anyhow::ensure!(
        !cfg!(debug_assertions),
        "开发构建不替换自身；请使用正式二进制"
    );
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => anyhow::bail!("当前平台请手动下载更新"),
    };
    let suffix = if cfg!(target_os = "windows") {
        ".zip"
    } else {
        ".tar.gz"
    };
    let bin = if cfg!(target_os = "windows") {
        "yank-client.exe"
    } else {
        "yank-client"
    };
    let status = self_update::backends::github::Update::configure()
        .repo_owner("ca-x")
        .repo_name("yank")
        .current_version(env!("CARGO_PKG_VERSION"))
        .target(platform)
        .asset_identifier(format!("{platform}{suffix}"))
        .bin_name(bin)
        .bin_path_in_archive(format!("yank-{{{{version}}}}-{platform}/{bin}"))
        .checksum_from_asset(format!("SHA256SUMS-{platform}.txt"))
        .timeout(std::time::Duration::from_secs(90))
        .show_output(false)
        .no_confirm(true)
        .build()?
        .update()?;
    Ok(if status.is_updated() {
        "新版本已下载并校验，下次启动 Yank 时生效"
    } else {
        "当前已是最新版本"
    }
    .into())
}
#[cfg(target_os = "android")]
pub fn update() -> Result<String> {
    anyhow::bail!("安卓请从发布页安装更新；测试签名 APK 可能需要先备份再重装")
}
