use anyhow::{Context, Result, ensure};
use nsh_updater::{Config, Download, install::Installation};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

const APP_ID: &str = "io.github.cax.yank";
const MARKER: &str = "yank-portable.json";
const MANAGED: &str =
    "由软件包管理器安装或安装来源未确认，请使用原安装渠道更新；应用内更新已禁用。";
const DEVELOPMENT: &str = "开发构建不替换自身；请使用正式二进制";
static UPDATING: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

fn managed_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/").to_lowercase();
    [
        "/usr/",
        "/bin/",
        "/sbin/",
        "/opt/",
        "/nix/",
        "/snap/",
        "/var/lib/flatpak/",
    ]
    .iter()
    .any(|prefix| normalized.starts_with(prefix))
        || [
            "/cellar/",
            "/caskroom/",
            "/homebrew/",
            "/scoop/",
            "/chocolatey/",
            "/winget/",
            "/windowsapps/",
            "/program files/",
            "/program files (x86)/",
            "/.local/share/flatpak/",
        ]
        .iter()
        .any(|segment| normalized.contains(segment))
}

fn owns_file(executable: &Path) -> bool {
    if executable
        .parent()
        .is_some_and(|root| root.join("install.json").is_file())
    {
        return true;
    }
    if std::env::var_os("FLATPAK_ID").is_some() || std::env::var_os("SNAP").is_some() {
        return true;
    }
    #[cfg(target_os = "linux")]
    for (program, args) in [
        ("dpkg-query", &["--search"][..]),
        ("rpm", &["-qf"][..]),
        ("pacman", &["-Qo"][..]),
    ] {
        if std::process::Command::new(program)
            .args(args)
            .arg(executable)
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return true;
        }
    }
    #[cfg(target_os = "windows")]
    for registry in [
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKLM\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        if std::process::Command::new("reg.exe")
            .args(["query", registry, "/s", "/f", "Yank", "/d"])
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return true;
        }
    }
    #[cfg(target_os = "macos")]
    if std::process::Command::new("/usr/sbin/pkgutil")
        .arg("--file-info")
        .arg(executable)
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("pkgid:"))
    {
        return true;
    }
    let _ = executable;
    false
}

fn portable_installation(executable: &Path) -> Result<Installation> {
    ensure!(!managed_path(executable), MANAGED);
    ensure!(!owns_file(executable), MANAGED);
    let root = executable.parent().context(MANAGED)?;
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join(MARKER)).context(MANAGED)?)?;
    ensure!(
        marker["id"] == APP_ID && marker["channel"] == "portable",
        MANAGED
    );
    ensure!(
        marker["executable"].as_str() == executable.file_name().and_then(|name| name.to_str()),
        MANAGED
    );
    Installation::directory(root, executable).context(MANAGED)
}

pub fn unavailable_reason() -> Option<&'static str> {
    let Ok(executable) = std::env::current_exe().and_then(|path| path.canonicalize()) else {
        return Some(MANAGED);
    };
    if portable_installation(&executable).is_err() {
        return Some(MANAGED);
    }
    cfg!(debug_assertions).then_some(DEVELOPMENT)
}

fn config() -> Result<Config> {
    Ok(Config::parse(&serde_json::to_vec(&serde_json::json!({
        "id": APP_ID,
        "version": env!("CARGO_PKG_VERSION"),
        "github": "ca-x/yank"
    }))?)?)
}

pub fn update() -> Result<String> {
    if UPDATING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Ok("更新正在进行".into());
    }
    struct Unlock;
    impl Drop for Unlock {
        fn drop(&mut self) {
            UPDATING.store(false, Ordering::Release);
        }
    }
    let _unlock = Unlock;
    if INSTALLED.load(Ordering::Acquire) {
        return Ok("新版本已下载并校验，下次启动 Yank 时生效".into());
    }
    let executable = std::env::current_exe()?.canonicalize()?;
    let installation = portable_installation(&executable)?;
    ensure!(!cfg!(debug_assertions), DEVELOPMENT);
    ensure!(
        installation.can_install(),
        "安装目录不可写，请使用原安装渠道更新"
    );
    let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86_64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x86_64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x86_64",
        _ => anyhow::bail!("当前平台请手动下载更新"),
    };
    let config = config()?;
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(300))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("Yank/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let mut manifest = Vec::new();
    client
        .get(config.manifest_url())
        .send()?
        .error_for_status()?
        .take(1_048_577)
        .read_to_end(&mut manifest)?;
    ensure!(manifest.len() <= 1_048_576, "更新清单过大");
    let Some(available) = config.check(&manifest, None, platform)? else {
        return Ok("当前已是最新版本".into());
    };
    let checksum = available.sha256.as_deref().context("更新校验和无效")?;
    ensure!(
        checksum.len() == 64 && hex::decode(checksum).is_ok(),
        "更新校验和无效"
    );
    ensure!(
        available
            .size
            .is_some_and(|size| size > 0 && size <= 536_870_912),
        "更新文件大小无效"
    );
    let mut response = client.get(&available.url).send()?.error_for_status()?;
    let mut archive = tempfile::NamedTempFile::new()?;
    let mut verification = Download::new(checksum, available.size);
    let mut buffer = [0u8; 65_536];
    loop {
        let received = response.read(&mut buffer)?;
        if received == 0 {
            break;
        }
        verification.update(&buffer[..received])?;
        archive.write_all(&buffer[..received])?;
    }
    verification.finish()?;
    archive.flush()?;
    portable_installation(&executable)?;
    installation.remove_leftovers();
    installation.install(archive.path())?;
    INSTALLED.store(true, Ordering::Release);
    Ok("新版本已下载并校验，下次启动 Yank 时生效".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_manager_paths_are_never_portable() {
        for path in [
            "/usr/bin/yank-client",
            "/opt/yank/yank-client",
            "/home/linuxbrew/.linuxbrew/Cellar/yank/1/yank-client",
            "/Users/me/homebrew/bin/yank-client",
            "C:\\Users\\me\\scoop\\apps\\yank\\current\\yank-client.exe",
            "C:\\Program Files\\Yank\\yank-client.exe",
            "C:\\Users\\me\\AppData\\Local\\Microsoft\\WinGet\\Packages\\Yank\\yank-client.exe",
            "/nix/store/hash-yank/bin/yank-client",
        ] {
            assert!(managed_path(Path::new(path)), "{path}");
        }
        assert!(!managed_path(Path::new(
            "/home/me/Applications/yank/yank-client"
        )));
    }

    #[test]
    fn unknown_installations_fail_closed_and_portable_needs_matching_marker() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("yank-client");
        fs::write(&executable, b"test").unwrap();
        assert!(portable_installation(&executable).is_err());
        let marker = directory.path().join(MARKER);
        fs::write(
            &marker,
            r#"{"id":"io.github.cax.yank","channel":"portable","executable":"yank-client"}"#,
        )
        .unwrap();
        assert!(portable_installation(&executable).is_ok());
        let scoop_marker = directory.path().join("install.json");
        fs::write(&scoop_marker, "{}").unwrap();
        assert!(portable_installation(&executable).is_err());
        fs::remove_file(scoop_marker).unwrap();
        fs::write(
            &marker,
            r#"{"id":"other","channel":"portable","executable":"yank-client"}"#,
        )
        .unwrap();
        assert!(portable_installation(&executable).is_err());
    }

    #[test]
    fn nsh_manifest_rejects_other_apps_and_accepts_versioned_archive() {
        let config = config().unwrap();
        let mut manifest = serde_json::json!({
            "id": APP_ID, "version": "99.0.0", "platforms": {
                "linux-x86_64": {"url":"https://github.com/ca-x/yank/releases/download/v99.0.0/yank.tar.gz", "sha256":"ab".repeat(32), "size":123}
            }
        });
        assert!(
            config
                .check(
                    &serde_json::to_vec(&manifest).unwrap(),
                    None,
                    "linux-x86_64"
                )
                .unwrap()
                .is_some()
        );
        manifest["id"] = "other.app".into();
        assert!(
            config
                .check(
                    &serde_json::to_vec(&manifest).unwrap(),
                    None,
                    "linux-x86_64"
                )
                .is_err()
        );
        let bytes = b"download";
        let mut download = Download::new(&"00".repeat(32), Some(bytes.len() as u64));
        download.update(bytes).unwrap();
        assert!(download.finish().is_err());
    }
}
