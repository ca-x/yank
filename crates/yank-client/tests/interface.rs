use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;
#[test]
fn offline_client_exposes_history_and_settings_without_login() -> Result<(), String> {
    let mut app = Headless::start(EngineConfig::new(Root::of::<yank_client::App>()))
        .map_err(|e| e.to_string())?;
    app.wait_for("全部记录")?;
    app.wait_for("复制过的内容，会留在这里")?;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("上传并保存在服务端")?;
    assert!(app.shows("隐私与备份"));
    Ok(())
}
#[test]
fn mobile_navigation_keeps_pairing_independent_of_accounts() -> Result<(), String> {
    let mut app = Headless::start(EngineConfig::new(Root::of::<yank_client::App>()))
        .map_err(|e| e.to_string())?;
    app.wait_for("全部记录")?;
    app.wait_for("前台手动记录 · 无需账号")?;
    assert!(!app.shows("暂停记录"));
    assert!(!app.shows("继续记录"));
    app.press("设备")?;
    app.wait_for("使用设备码配对")?;
    app.wait_for("已绑定设备")?;
    app.press("队列")?;
    app.wait_for("复制下一条")?;
    Ok(())
}

use nsh_core::{
    Checked,
    platform::Os,
    prelude::{component, px, size},
};
use std::time::Duration;
use yank_core::{Clip, ClipFormat, Preferences, SavedView, Store};

struct Fixture {
    app: Headless,
    store: Store,
    _directory: tempfile::TempDir,
}
impl Fixture {
    fn start(width: f32, desktop: bool, clips: Vec<Clip>) -> Result<Self, String> {
        Self::start_with_clock(width, desktop, clips, false)
    }
    fn start_with_clock(
        width: f32,
        desktop: bool,
        clips: Vec<Clip>,
        manual_clock: bool,
    ) -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let app_dir = directory.path().join("default");
        std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        let store = Store::open(app_dir.join("yank-v2.sqlite3")).map_err(|e| e.to_string())?;
        // Disable OS capture and account sync in seeded headless fixtures.
        store
            .save_preferences(&Preferences {
                capture: false,
                mode: "direct".into(),
                ..Default::default()
            })
            .map_err(|e| e.to_string())?;
        for clip in clips {
            store.capture(clip, "fixture").map_err(|e| e.to_string())?;
        }
        store
            .save_view(SavedView {
                id: "alpha".into(),
                name: "Alpha 智能列表".into(),
                query: "alpha".into(),
                filter: "all".into(),
                days: 0,
                icon: "list".into(),
            })
            .map_err(|e| e.to_string())?;
        let mut config = EngineConfig::new(Root::of::<yank_client::App>());
        config.data_dir = Some(directory.path().to_path_buf());
        if manual_clock {
            config.clock = nsh_core::time::Clock::manual(std::time::SystemTime::UNIX_EPOCH);
        }
        config.viewport = size(px(width), px(900.0));
        config.environment.os = if desktop { Os::Linux } else { Os::Ios };
        let mut app = Headless::start(config).map_err(|e| e.to_string())?;
        app.wait_for("全部记录")?;
        wait_screen(&mut app, "seeded history", |screen| {
            screen.matches("Alpha clip").unwrap_or(false)
        })?;
        Ok(Self {
            app,
            store,
            _directory: directory,
        })
    }
}
fn clips() -> Vec<Clip> {
    let mut alpha = Clip::text("fixture", "alpha saved");
    alpha.description = "Alpha clip".into();
    alpha.collection = "工作资料".into();
    let mut beta = Clip::text("fixture", "beta saved");
    beta.description = "Beta clip".into();
    vec![alpha, beta]
}

#[test]
fn detail_opens_read_only_and_discard_restores_saved_content() -> Result<(), String> {
    let mut fixture = Fixture::start(1120.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    app.wait_for("粘贴到原应用")?;
    assert!(!app.shows("编辑文字"));
    assert!(!app.shows("复制草稿文字"));
    app.press("编辑")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    replace_input(app, "编辑文字", "unsaved draft")?;
    app.wait_for("保存编辑")?;
    app.wait_for("复制草稿文字")?;
    app.press("放弃修改")?;
    wait_screen(app, "reading mode after discard", |screen| {
        !screen.matches("编辑文字").unwrap_or(false)
    })?;
    app.press("编辑")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    assert_eq!(
        fixture.store.list("alpha", "all", 10, 0).unwrap()[0].text,
        "alpha saved"
    );
    Ok(())
}

#[test]
fn queue_removal_and_copy_undo_keep_the_item_addressable() -> Result<(), String> {
    let mut fixture = Fixture::start(390.0, false, clips())?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    app.wait_for("加入顺序复制")?;
    app.press("加入顺序复制")?;
    app.press("队列")?;
    app.wait_for("队列中还有 1 条")?;
    app.press("复制下一条")?;
    app.wait_for("队列中还有 0 条")?;
    app.press("撤销取出")?;
    app.wait_for("队列中还有 1 条")?;
    app.press("从队列移除")?;
    app.wait_for("队列中还有 0 条")?;
    assert_eq!(fixture.store.list("alpha", "all", 10, 0).unwrap().len(), 1);
    Ok(())
}

#[test]
fn settings_inputs_and_switches_fit_the_viewport() -> Result<(), String> {
    let mut fixture = Fixture::start(1180.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置与账号")?;
    app.wait_for("启用自动清理")?;
    for width in [1180.0, 390.0] {
        app.set_viewport(size(px(width), px(900.0)))?;
        let switch = app
            .screen()
            .press_target("启用自动清理")?
            .ok_or("missing cleanup switch")?;
        let nodes = app.screen().layout_nodes();
        assert!(
            nodes
                .iter()
                .any(|node| node.kind == nsh_core::ViewKind::Input)
        );
        for node in nodes.iter().filter(|node| {
            !node.hidden && (node.id == switch || node.kind == nsh_core::ViewKind::Input)
        }) {
            assert!(node.bounds.origin.x.0 >= 0.0, "settings control left edge");
            assert!(
                node.bounds.origin.x.0 + node.bounds.size.width.0 <= width + 0.1,
                "settings control {:?} overflows {width}: {:?}",
                node.id,
                node.bounds
            );
        }
    }
    Ok(())
}

#[test]
fn pseudo_language_keeps_settings_controls_in_bounds_and_user_titles_unchanged()
-> Result<(), String> {
    let mut fixture = Fixture::start(1180.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置与账号")?;
    app.wait_for("伪语言（布局测试）")?;
    app.press("伪语言（布局测试）")?;
    let settings = yank_core::i18n::pseudo_source_text("设置与账号");
    app.wait_for(&settings)?;
    for width in [1180.0, 390.0] {
        app.set_viewport(size(px(width), px(900.0)))?;
        for node in app
            .screen()
            .layout_nodes()
            .iter()
            .filter(|node| !node.hidden && node.kind == nsh_core::ViewKind::Input)
        {
            assert!(node.bounds.origin.x.0 >= 0.0);
            assert!(node.bounds.origin.x.0 + node.bounds.size.width.0 <= width + 0.1);
        }
    }
    app.set_viewport(size(px(1180.0), px(900.0)))?;
    app.press(&yank_core::i18n::pseudo_source_text("全部记录"))?;
    app.wait_for("Alpha clip")?;
    assert!(!app.shows("⟦Alpha clip⟧"));
    Ok(())
}
fn wait_input(app: &mut Headless, label: &str, value: &str) -> Result<(), String> {
    if matches!(label, "编辑文字" | "Edit text") && !app.shows(label) {
        app.wait_for(if label == "Edit text" {
            "Edit"
        } else {
            "编辑"
        })?;
        app.press(if label == "Edit text" {
            "Edit"
        } else {
            "编辑"
        })?;
    }
    app.wait_until("hydrated editor value", Duration::from_secs(3), |screen| {
        screen.input_text(label).ok().flatten() == Some(value)
    })
}

fn wait_screen(
    app: &mut Headless,
    what: &str,
    done: impl Fn(&nsh_testing::Screen) -> bool,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if done(app.screen()) {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!("{what}: UI condition still false after 3000 ms"));
        }
        let slice = remaining.min(Duration::from_millis(20));
        if app.clock().is_manual() {
            app.run_for(slice)?;
        } else {
            let _ = app.next_batch(slice);
        }
    }
}

// Persisted state can change without producing a new UI batch. Poll it on a
// bounded cadence and check once more at the deadline, rather than relying on
// Headless::wait_until's UI-event wakeups for an external database predicate.
fn wait_store(app: &mut Headless, what: &str, done: impl Fn() -> bool) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if done() {
            return Ok(());
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(format!(
                "{what}: database condition still false after 3000 ms"
            ));
        }
        let slice = remaining.min(Duration::from_millis(20));
        if app.clock().is_manual() {
            app.run_for(slice)?;
        } else {
            let _ = app.next_batch(slice);
        }
    }
}

fn replace_input(app: &mut Headless, label: &str, value: &str) -> Result<(), String> {
    app.clear(label)?;
    if !value.is_empty() {
        app.type_text(label, value)?;
    }
    Ok(())
}

#[component]
fn QuietWaitScreen() -> impl nsh_core::prelude::IntoElement {
    nsh_core::prelude::text("No UI updates")
}

#[component]
fn BackgroundPolledScreen() -> impl nsh_core::prelude::IntoElement {
    use nsh_core::prelude::*;
    let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = ready.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        writer.store(true, std::sync::atomic::Ordering::Release);
    });
    let shown = signal(false);
    set_interval(Duration::from_millis(20), move || {
        shown.set(ready.load(std::sync::atomic::Ordering::Acquire));
    });
    text(move || {
        if shown.get() {
            "Worker ready"
        } else {
            "Waiting for worker"
        }
    })
}

#[test]
fn manual_clock_keeps_polling_until_real_worker_has_published() -> Result<(), String> {
    let mut config = EngineConfig::new(Root::of::<BackgroundPolledScreen>());
    config.clock = nsh_core::time::Clock::manual(std::time::SystemTime::UNIX_EPOCH);
    let mut app = Headless::start(config).map_err(|error| error.to_string())?;
    wait_screen(&mut app, "real worker result", |screen| {
        screen.matches("Worker ready").unwrap_or(false)
    })
}

#[test]
fn persisted_state_wait_does_not_require_ui_updates() -> Result<(), String> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    for manual in [false, true] {
        let mut config = EngineConfig::new(Root::of::<QuietWaitScreen>());
        if manual {
            config.clock = nsh_core::time::Clock::manual(std::time::SystemTime::UNIX_EPOCH);
        }
        let mut app = Headless::start(config).map_err(|e| e.to_string())?;
        app.wait_for("No UI updates")?;
        let changed = Arc::new(AtomicBool::new(false));
        let writer = changed.clone();
        let task = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            writer.store(true, Ordering::SeqCst);
        });
        let result = wait_store(&mut app, "external state changed on a quiet screen", || {
            changed.load(Ordering::SeqCst)
        });
        task.join().map_err(|_| "external writer panicked")?;
        result?;
    }
    Ok(())
}

#[test]
fn quick_paste_keys_leave_settings_alone_and_escape_hides_history() -> Result<(), String> {
    let mut fixture = Fixture::start(1120.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置与账号")?;
    app.wait_for("账号登录（可选）")?;
    app.press_key("mod+1")?;
    assert!(
        app.window_state("main")?
            .ok_or("missing main window")?
            .visible,
        "history quick-paste bindings must be removed in settings"
    );
    app.press_key("escape")?;
    app.wait_for("搜索文字、标题或收藏板")?;
    assert!(
        app.window_state("main")?
            .ok_or("missing main window")?
            .visible,
        "Escape from settings returns to history first"
    );
    app.type_text("搜索文字、标题或收藏板", "alpha")?;
    app.press_key("escape")?;
    assert!(
        !app.window_state("main")?
            .ok_or("missing main window")?
            .visible,
        "Escape hides history even while search is focused"
    );
    assert_eq!(fixture.store.stats().map_err(|e| e.to_string())?.clips, 2);
    Ok(())
}

#[test]
fn headless_desktop_849_850_switches_navigation_and_detail_behavior() -> Result<(), String> {
    let mut fixture = Fixture::start(849.0, true, clips())?;
    let app = &mut fixture.app;
    assert!(!app.shows("yank"));
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    assert!(!app.shows("Beta clip"), "849 uses a dedicated detail page");
    app.back()?;
    app.wait_for("Beta clip")?;
    app.set_viewport(size(px(850.0), px(900.0)))?;
    app.wait_for("yank")?;
    app.press("Beta clip")?;
    wait_input(app, "编辑文字", "beta saved")?;
    assert!(
        app.shows("Alpha clip"),
        "850 keeps history beside the editor"
    );
    app.set_viewport(size(px(849.0), px(900.0)))?;
    assert!(!app.shows("yank"));
    Ok(())
}

#[test]
fn headless_phone_keeps_each_draft_across_back_selection_and_copy() -> Result<(), String> {
    let mut fixture = Fixture::start(390.0, false, clips())?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    app.clear("编辑文字")?;
    app.type_text("编辑文字", "alpha draft")?;
    app.wait_for("未保存 · 切换记录或返回后草稿仍保留")?;
    app.press("复制")?;
    assert_eq!(app.screen().clipboard(), Some("alpha saved"));
    app.press("复制草稿文字")?;
    assert_eq!(app.screen().clipboard(), Some("alpha draft"));
    app.set_environment("software keyboard visible", |env| {
        env.keyboard_height = px(280.0)
    })?;
    app.back()?;
    assert!(
        app.shows("内容详情"),
        "back first dismisses the keyboard without leaving the draft"
    );
    app.set_environment("software keyboard hidden", |env| {
        env.keyboard_height = px(0.0)
    })?;
    app.back()?;
    app.press("Beta clip")?;
    wait_input(app, "编辑文字", "beta saved")?;
    app.clear("收藏板名称")?;
    app.type_text("收藏板名称", "Beta draft board")?;
    app.back()?;
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha draft")?;
    app.press("保存编辑")?;
    wait_screen(app, "draft saved", |screen| {
        screen.press_target("保存编辑").ok().flatten().is_none()
    })?;
    app.back()?;
    app.press("Beta clip")?;
    wait_input(app, "收藏板名称", "Beta draft board")?;
    Ok(())
}

#[test]
fn headless_narrow_settings_exposes_exclusive_transport_and_saved_preferences() -> Result<(), String>
{
    let mut fixture = Fixture::start(849.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("设备传输方式")?;
    assert_eq!(
        app.screen()
            .accessibility_state("role:radio[仅直连]")?
            .checked,
        Some(Checked::True)
    );
    assert_eq!(
        app.screen()
            .accessibility_state("role:radio[默认中继]")?
            .checked,
        Some(Checked::False)
    );
    app.press("自动 NAT / 中继保底")?;
    app.wait_until("exclusive transport", Duration::from_secs(3), |s| {
        s.accessibility_state("role:radio[自动 NAT / 中继保底]")
            .ok()
            .and_then(|s| s.checked)
            == Some(Checked::True)
    })?;
    assert_eq!(
        app.screen()
            .accessibility_state("role:radio[仅直连]")?
            .checked,
        Some(Checked::False)
    );
    wait_store(app, "saved transport", || {
        fixture.store.preferences().unwrap().mode == "auto"
    })?;
    for label in [
        "后续远端变更写入系统剪贴板",
        "排除的应用（每行一个）",
        "每日自动备份",
        "同时清理超过天数的记录（0 为不按时间清理）",
        "显示面板快捷键",
        "队列下一条快捷键",
        "暂停记录快捷键",
        "悬浮卡片快捷键",
        "静默启动",
        "面板位置",
        "登录系统时启动",
    ] {
        assert!(app.shows(label), "missing preference {label}");
    }
    assert!(!app.shows("自动检查并下载更新"));
    assert!(
        app.shows("由软件包管理器安装或安装来源未确认，请使用原安装渠道更新；应用内更新已禁用。")
    );
    app.press("后续远端变更写入系统剪贴板")?;
    wait_store(app, "remote apply disabled", || {
        !fixture.store.preferences().unwrap().apply_remote
    })?;
    app.set_viewport(size(px(390.0), px(900.0)))?;
    for label in ["默认中继", "仅直连", "自动 NAT / 中继保底"] {
        let id = app
            .screen()
            .press_target(label)?
            .ok_or_else(|| format!("transport unavailable: {label}"))?;
        let nodes = app.screen().layout_nodes();
        let node = nodes
            .iter()
            .find(|node| node.id == id)
            .ok_or("missing radio layout")?;
        assert!(node.bounds.origin.x.0 >= 0.0);
        assert!(
            node.bounds.origin.x.0 + node.bounds.size.width.0 <= 390.1,
            "radio overflows phone width: {label}"
        );
        assert!(node.bounds.size.height.0 >= 48.0);
    }
    Ok(())
}

fn wait_settings_snapshot(app: &mut Headless) -> Result<(), String> {
    app.run_for(Duration::from_millis(450))
}

#[test]
fn headless_settings_regex_draft_survives_slow_typing_rejection_and_navigation()
-> Result<(), String> {
    let mut fixture = Fixture::start_with_clock(849.0, true, clips(), true)?;
    let app = &mut fixture.app;
    let label = "排除内容的正则表达式（可留空）";
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for(label)?;
    assert!(app.screen().press_target("保存设置修改")?.is_none());
    app.type_text(label, "[")?;
    wait_settings_snapshot(app)?;
    let backup_daily = fixture.store.preferences().unwrap().backup_daily;
    app.press("每日自动备份")?;
    wait_store(app, "toggle snapshot while regex is unfinished", || {
        fixture.store.preferences().unwrap().backup_daily != backup_daily
    })?;
    wait_settings_snapshot(app)?;
    assert_eq!(fixture.store.preferences().unwrap().exclude_pattern, "");
    assert_eq!(app.screen().input_text(label)?, Some("["));
    assert!(
        !app.screen()
            .texts()
            .iter()
            .any(|s| s.contains("regex parse error")),
        "typing an unfinished expression must not submit it"
    );
    app.press("保存设置修改")?;
    app.wait_until("invalid regex rejected", Duration::from_secs(3), |s| {
        s.texts().iter().any(|s| s.contains("regex parse error"))
    })?;
    assert_eq!(fixture.store.preferences().unwrap().exclude_pattern, "");
    assert_eq!(app.screen().input_text(label)?, Some("["));
    assert!(
        app.screen().enabled("保存设置修改")?,
        "a rejected draft must remain available to retry"
    );
    app.back()?;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    wait_input(app, label, "[")?;
    app.type_text(label, "ab]")?;
    wait_settings_snapshot(app)?;
    assert_eq!(fixture.store.preferences().unwrap().exclude_pattern, "");
    app.press("保存设置修改")?;
    wait_store(app, "complete regex saved", || {
        fixture.store.preferences().unwrap().exclude_pattern == "[ab]"
    })?;
    wait_settings_snapshot(app)?;
    assert_eq!(app.screen().input_text(label)?, Some("[ab]"));
    wait_screen(app, "saved settings bar disappears", |screen| {
        screen.press_target("保存设置修改").ok().flatten().is_none()
    })?;
    Ok(())
}

#[test]
fn headless_settings_text_drafts_do_not_leak_into_toggle_saves() -> Result<(), String> {
    let mut fixture = Fixture::start_with_clock(849.0, true, clips(), true)?;
    let app = &mut fixture.app;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("显示面板快捷键")?;
    let original = fixture.store.preferences().unwrap();
    app.press("Ctrl+Shift+V")?;
    app.wait_for("请按组合键，Esc 取消")?;
    app.press_key("mod+shift+k")?;
    wait_settings_snapshot(app)?;
    assert_eq!(
        fixture.store.preferences().unwrap().hotkey_show,
        original.hotkey_show
    );
    let fields = [
        (
            "中继地址（留空使用服务器提供的地址）",
            "保存中继地址",
            "ws://127.0.0.1:9",
            (|p: &Preferences| p.relay_url.clone()) as fn(&Preferences) -> String,
        ),
        (
            "Ditto 监听地址",
            "保存 Ditto 监听地址",
            "127.0.0.1:23444",
            |p: &Preferences| p.ditto_listen.clone(),
        ),
        (
            "Ditto 共享密码",
            "保存 Ditto 共享密码",
            "secret",
            |p: &Preferences| p.ditto_password.clone(),
        ),
        (
            "远端 Ditto 地址（主机:端口）",
            "保存远端 Ditto 地址",
            "host:23443",
            |p: &Preferences| p.ditto_remote.clone(),
        ),
        (
            "排除的应用（每行一个）",
            "保存排除的应用",
            "Editor\nTerminal",
            |p: &Preferences| p.excluded_apps.clone(),
        ),
    ];
    for (label, _, value, get) in fields {
        app.clear(label)?;
        let first = &value[..1];
        app.type_text(label, first)?;
        wait_settings_snapshot(app)?;
        assert_eq!(
            get(&fixture.store.preferences().unwrap()),
            get(&original),
            "prefix of {label} reached the database"
        );
        assert_eq!(app.screen().input_text(label)?, Some(first));
        app.type_text(label, &value[1..])?;
    }
    app.press("每日自动备份")?;
    wait_store(app, "toggle saved immediately", || {
        fixture.store.preferences().unwrap().backup_daily != original.backup_daily
    })?;
    wait_settings_snapshot(app)?;
    for (label, _, value, get) in fields {
        assert_eq!(
            get(&fixture.store.preferences().unwrap()),
            get(&original),
            "toggle saved the draft for {label}"
        );
        assert_eq!(app.screen().input_text(label)?, Some(value));
    }
    assert_eq!(
        fixture.store.preferences().unwrap().hotkey_show,
        original.hotkey_show
    );
    assert!(app.shows("Ctrl+Shift+K"));
    app.press("保存设置修改")?;
    for (_, _, value, get) in fields {
        wait_store(app, "grouped text preference saved", || {
            get(&fixture.store.preferences().unwrap()) == value
        })?;
    }
    wait_store(app, "complete shortcut saved", || {
        fixture.store.preferences().unwrap().hotkey_show == "mod+shift+k"
    })?;
    Ok(())
}

#[test]
fn headless_phone_opens_existing_smart_lists_and_boards() -> Result<(), String> {
    let mut fixture = Fixture::start(390.0, false, clips())?;
    let app = &mut fixture.app;
    app.press("列表")?;
    app.wait_for("Alpha 智能列表")?;
    app.wait_for("工作资料")?;
    app.press("Alpha 智能列表")?;
    app.wait_for("Alpha clip")?;
    app.wait_for_gone("Beta clip", Duration::from_secs(3))?;
    app.press("列表")?;
    app.press("工作资料")?;
    app.wait_for("Alpha clip")?;
    assert!(!app.shows("Beta clip"));
    Ok(())
}

#[test]
fn headless_binary_fallback_can_change_collection_without_changing_formats() -> Result<(), String> {
    let mut records = clips();
    records[0].formats.push(ClipFormat {
        format: "native-binary".into(),
        mime: None,
        data: vec![0, 255, 1, 128],
    });
    let original = records[0].clone();
    let mut fixture = Fixture::start(390.0, false, records)?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    assert!(app.screen().press_target("复制草稿文字")?.is_none());
    app.clear("收藏板名称")?;
    app.type_text("收藏板名称", "Binary board")?;
    app.press("保存编辑")?;
    wait_store(app, "binary collection persisted", || {
        fixture.store.get(&original.id).unwrap().unwrap().collection == "Binary board"
    })?;
    let saved = fixture
        .store
        .get(&original.id)
        .map_err(|e| e.to_string())?
        .unwrap();
    assert_eq!(saved.text, original.text);
    assert_eq!(saved.formats, original.formats);
    Ok(())
}

#[test]
fn headless_markdown_blocks_remote_images_and_renders_html_as_text() -> Result<(), String> {
    let mut records = clips();
    records[0].text="![tracking](https://example.invalid/tracker.png)\n\n<img src=\"https://example.invalid/html.png\">".into();
    records[0].formats = vec![ClipFormat::text(&records[0].text)];
    let mut fixture = Fixture::start(390.0, false, records)?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    app.wait_for("Markdown / 代码预览")?;
    app.press("Markdown / 代码预览")?;
    app.wait_for("图片已阻止加载 · 预览不会请求远端图片")?;
    assert!(app.shows("<img src=\"https://example.invalid/html.png\">"));
    assert!(
        !app.screen()
            .layout_nodes()
            .iter()
            .any(|node| node.kind == nsh_core::ViewKind::Image),
        "untrusted preview must create no image views"
    );
    app.back()?;
    app.wait_for("内容详情")?;
    Ok(())
}

#[test]
fn headless_phone_collects_only_new_local_captures_and_deduplicates_queue() -> Result<(), String> {
    let mut fixture = Fixture::start(390.0, false, clips())?;
    let app = &mut fixture.app;
    app.press("队列")?;
    app.press("自动收集本机剪贴板")?;
    app.wait_for("队列中还有 0 条")?;
    app.set_clipboard(Some("fresh local capture".into()));
    app.press("记录剪贴板")?;
    app.wait_for("队列中还有 1 条")?;
    app.press("记录剪贴板")?;
    app.wait_for("队列中还有 1 条")?;
    app.press("自动收集本机剪贴板")?;
    app.set_clipboard(Some("capture after collecting stopped".into()));
    app.press("记录剪贴板")?;
    app.wait_for("队列中还有 1 条")?;
    app.press("复制下一条")?;
    assert_eq!(app.screen().clipboard(), Some("fresh local capture"));
    app.wait_for("队列中还有 0 条")?;
    Ok(())
}

#[test]
fn headless_pending_login_disables_duplicate_login_import_and_ditto() -> Result<(), String> {
    use std::{io::Write, net::TcpListener, sync::mpsc};
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let address = listener.local_addr().map_err(|e| e.to_string())?;
    let (release, held) = mpsc::channel();
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = held.recv_timeout(Duration::from_secs(8));
            let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    let mut fixture = Fixture::start(390.0, false, clips())?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.type_text(
        "服务器地址，例如 https://yank.example.com",
        &format!("http://{address}"),
    )?;
    app.type_text("用户名", "fixture")?;
    app.type_text("密码", "password")?;
    app.press("登录并关联此设备")?;
    app.wait_for("正在登录…")?;
    assert!(app.screen().press_target("登录并关联此设备")?.is_none());
    assert!(app.screen().press_target("导入备份")?.is_none());
    app.press("记录")?;
    app.press("Alpha clip")?;
    assert!(app.screen().press_target("发送到远端 Ditto")?.is_none());
    let _ = release.send(());
    server
        .join()
        .map_err(|_| "fixture HTTP server panicked".to_string())?;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_until(
        "form error and footer error",
        Duration::from_secs(3),
        |screen| {
            screen
                .count("text:/操作失败：/")
                .is_ok_and(|count| count >= 2)
        },
    )?;
    Ok(())
}

fn recovery_fixture() -> Result<(tempfile::TempDir, std::path::PathBuf, EngineConfig), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let backups = directory.path().join("default/backups");
    std::fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
    let backup = backups.join("yank-20000.json");
    std::fs::write(&backup, "invalid backup").map_err(|e| e.to_string())?;
    let mut config = EngineConfig::new(Root::of::<yank_client::App>());
    config.data_dir = Some(directory.path().to_path_buf());
    config.viewport = size(px(390.0), px(900.0));
    Ok((directory, backup, config))
}

#[test]
fn headless_failed_recovery_preserves_backup_and_offers_retry() -> Result<(), String> {
    let (_directory, backup, config) = recovery_fixture()?;
    let mut app = Headless::start(config).map_err(|e| e.to_string())?;
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("恢复历史".into()));
    app.wait_until("recovery prompt", Duration::from_secs(3), |s| {
        s.dialogs()
            .iter()
            .any(|d| d.title.as_deref() == Some("本地历史文件不存在，恢复自动备份？"))
    })?;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("重试恢复自动备份")?;
    assert_eq!(
        std::fs::read_to_string(&backup).map_err(|e| e.to_string())?,
        "invalid backup"
    );
    let exported = Store::memory().map_err(|e| e.to_string())?;
    exported
        .capture(Clip::text("fixture", "recovered history"), "fixture")
        .map_err(|e| e.to_string())?;
    std::fs::write(&backup, exported.export().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    app.press("重试恢复自动备份")?;
    app.wait_for_gone("重试恢复自动备份", Duration::from_secs(3))?;
    app.press("记录")?;
    app.wait_for("recovered history")?;
    Ok(())
}

#[test]
fn headless_desktop_floating_card_reopens_after_native_close() -> Result<(), String> {
    let records = clips();
    let id = format!("floating:{}", records[0].id);
    let mut fixture = Fixture::start(850.0, true, records)?;
    let app = &mut fixture.app;
    app.press("Alpha clip")?;
    wait_input(app, "编辑文字", "alpha saved")?;
    app.press("悬浮为置顶卡片")?;
    app.wait_for("悬浮预览 · 已保存内容 · 保持置顶")?;
    assert!(app.window_state(&id)?.is_some());
    app.press_window_close(&id)?;
    assert!(app.window_state(&id)?.is_none());
    app.press("悬浮为置顶卡片")?;
    app.wait_for("悬浮预览 · 已保存内容 · 保持置顶")?;
    assert!(
        app.window_state(&id)?.is_some(),
        "closed window handles must not suppress reopening"
    );
    Ok(())
}

#[test]
fn headless_close_cancel_preserves_ask_and_minimize_is_saved_before_hiding() -> Result<(), String> {
    let mut fixture = Fixture::start(850.0, true, clips())?;
    let app = &mut fixture.app;
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("取消".into()));
    app.press_window_close("main")?;
    assert_eq!(
        fixture.store.preferences().unwrap().close_behavior,
        yank_core::CloseBehavior::Ask
    );
    assert!(!app.window_state("main")?.unwrap().minimized);
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("最小化并继续运行".into()));
    app.press_window_close("main")?;
    wait_store(app, "close behavior saved", || {
        fixture.store.preferences().unwrap().close_behavior == yank_core::CloseBehavior::Minimize
    })?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while !app
        .window_state("main")?
        .is_some_and(|state| state.minimized)
    {
        if std::time::Instant::now() >= deadline {
            return Err("window was not minimized after saving close behavior".into());
        }
        let _ = app.next_batch(Duration::from_millis(20));
    }
    app.on_dialog(|_| panic!("remembered close behavior must not ask again"));
    app.press_window_close("main")?;
    Ok(())
}

#[test]
fn headless_tray_click_places_panel_at_anchor_and_falls_back_without_coordinates()
-> Result<(), String> {
    use nsh_core::{
        Bounds, Command, EventKind, EventPayload, PayloadValue, ROOT_ID,
        desktop::{DesktopCommand, Display, DisplayId},
        geometry::point,
    };
    let mut fixture = Fixture::start(849.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("托盘附近")?;
    app.press("托盘附近")?;
    wait_store(app, "tray position persisted", || {
        fixture.store.preferences().unwrap().panel_position == "tray"
    })?;
    let click = app
        .screen()
        .commands()
        .iter()
        .find_map(|(_, command)| match command {
            Command::Desktop(command) => match &**command {
                DesktopCommand::SetTray(Some(tray)) => Some(tray.click),
                _ => None,
            },
            _ => None,
        })
        .ok_or("no native tray click registration")?;
    let area = Bounds::new(point(px(-1600.0), px(24.0)), size(px(1600.0), px(976.0)));
    let primary = Bounds::new(point(px(0.0), px(0.0)), size(px(1920.0), px(1080.0)));
    app.engine().control().set_displays(vec![
        Display::new(DisplayId(1), primary, primary, true),
        Display::new(
            DisplayId(2),
            Bounds::new(point(px(-1600.0), px(0.0)), size(px(1600.0), px(1000.0))),
            area,
            false,
        ),
    ]);
    let anchor = Bounds::new(point(px(-80.0), px(0.0)), size(px(24.0), px(24.0)));
    app.engine().control().dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(f64::from(click))),
            ("bounds", PayloadValue::Bounds(anchor)),
        ]),
    );
    app.wait(Duration::from_millis(150))?;
    let bounds = app
        .window_state("main")?
        .ok_or("missing main window")?
        .bounds;
    assert_eq!(
        bounds.origin.x,
        px(-960.0),
        "panel clamps at the anchored monitor's right edge"
    );
    assert_eq!(bounds.origin.y, px(32.0), "panel opens below the top tray");
    assert_eq!(bounds.size.width, px(960.0));
    let bottom = Bounds::new(point(px(-80.0), px(976.0)), size(px(24.0), px(24.0)));
    app.engine().control().dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![
            ("id", PayloadValue::Number(f64::from(click))),
            ("bounds", PayloadValue::Bounds(bottom)),
        ]),
    );
    app.wait(Duration::from_millis(150))?;
    assert_eq!(
        app.window_state("main")?.unwrap().bounds.origin.y,
        px(288.0),
        "bottom tray opens upward"
    );
    app.engine().control().dispatch_event(
        ROOT_ID,
        EventKind::Menu,
        EventPayload::Map(vec![("id", PayloadValue::Number(f64::from(click)))]),
    );
    app.wait(Duration::from_millis(150))?;
    let centered = app.window_state("main")?.unwrap().bounds;
    assert_eq!(
        centered.origin,
        point(px(480.0), px(200.0)),
        "missing tray coordinates use the primary monitor center"
    );
    Ok(())
}

#[test]
fn headless_language_switch_rebinds_controls_and_preserves_user_content() -> Result<(), String> {
    let mut records = clips();
    records[0].text = "图片".into();
    records[0].collection = "文字".into();
    let mut fixture = Fixture::start(1120.0, true, records)?;
    // A renamed seeded list is user content, even if it matches another UI label.
    let mut renamed = fixture
        .store
        .views()
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|v| v.id == "default-images")
        .ok_or("missing seeded view")?;
    renamed.name = "设置".into();
    fixture
        .store
        .save_view(renamed)
        .map_err(|e| e.to_string())?;
    let app = &mut fixture.app;
    app.press("设置与账号")?;
    app.wait_for("保留最新的未收藏记录总数")?;
    replace_input(app, "保留最新的未收藏记录总数", "4321")?;
    app.press("English")?;
    app.wait_for("Settings and account")?;
    app.wait_for("Enable automatic cleanup")?;
    app.wait_for("Device name")?;
    wait_input(app, "Keep latest unpinned records (total)", "4321")?;
    wait_store(app, "language persisted", || {
        fixture.store.preferences().unwrap().language == "en"
    })?;
    app.press("All history")?;
    app.wait_for("Alpha clip")?;
    app.press("Alpha clip")?;
    app.wait_for("Copy")?;
    wait_input(app, "Edit text", "图片")?;
    wait_input(app, "Board name", "文字")?;
    assert!(app.shows("设置"), "renamed seeded list must stay unchanged");
    app.press("Settings and account")?;
    app.wait_for("中文")?;
    app.press("中文")?;
    app.wait_for("上传并保存在服务端")?;
    wait_store(app, "Chinese persisted", || {
        fixture.store.preferences().unwrap().language == "zh"
    })?;
    Ok(())
}

#[test]
fn headless_cleanup_rejects_invalid_drafts_and_requires_destructive_confirmation()
-> Result<(), String> {
    let mut fixture = Fixture::start(849.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.wait_for("启用自动清理")?;
    app.press("启用自动清理")?;
    replace_input(app, "保留最新的未收藏记录总数", "1")?;
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("取消".into()));
    app.press("保存清理设置")?;
    let _ = app.next_batch(Duration::from_millis(200));
    assert!(!fixture.store.preferences().unwrap().cleanup.enabled);
    assert_eq!(fixture.store.stats().unwrap().clips, 2);
    for value in ["0", "-1", "1000001", "1.5", ""] {
        replace_input(app, "保留最新的未收藏记录总数", value)?;
        app.press("保存清理设置")?;
        app.wait_for("清理设置无效：条数须为 1 至 1,000,000 的整数，天数须为非负整数。")?;
        assert!(!fixture.store.preferences().unwrap().cleanup.enabled);
        assert_eq!(fixture.store.stats().unwrap().clips, 2);
    }
    replace_input(app, "保留最新的未收藏记录总数", "2")?;
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("应用清理设置".into()));
    app.press("保存清理设置")?;
    wait_store(app, "cleanup enabled", || {
        fixture.store.preferences().unwrap().cleanup.enabled
    })?;
    replace_input(app, "保留最新的未收藏记录总数", "1")?;
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("取消".into()));
    app.press("保存清理设置")?;
    let _ = app.next_batch(Duration::from_millis(200));
    assert_eq!(fixture.store.preferences().unwrap().cleanup.max_items, 2);
    assert_eq!(fixture.store.stats().unwrap().clips, 2);
    app.on_dialog(|_| nsh_testing::DialogAnswer::Press("应用清理设置".into()));
    app.press("保存清理设置")?;
    wait_store(app, "immediate cleanup after save", || {
        fixture.store.stats().unwrap().clips == 1
    })?;
    app.press("记录")?;
    app.wait_for("text:/^1 条结果/")?;
    Ok(())
}

#[test]
fn headless_per_type_cleanup_draft_does_not_overwrite_policy_when_invalid() -> Result<(), String> {
    let mut fixture = Fixture::start(849.0, true, clips())?;
    let app = &mut fixture.app;
    app.press("设置")?;
    app.wait_for("账号登录（可选）")?;
    app.press("按类型分别保留")?;
    for label in [
        "文字保留条数",
        "链接保留条数",
        "图片保留条数",
        "文件保留条数",
        "代码保留条数",
        "颜色保留条数",
    ] {
        app.wait_for(label)?;
    }
    replace_input(app, "颜色保留条数", "0")?;
    app.press("保存清理设置")?;
    app.wait_for("清理设置无效：条数须为 1 至 1,000,000 的整数，天数须为非负整数。")?;
    assert_eq!(
        fixture.store.preferences().unwrap().cleanup.mode,
        yank_core::CleanupMode::Total
    );
    replace_input(app, "颜色保留条数", "15")?;
    replace_input(app, "同时清理超过天数的记录（0 为不按时间清理）", "-1")?;
    app.press("保存清理设置")?;
    assert_eq!(fixture.store.preferences().unwrap().retention_days, 0);
    replace_input(app, "同时清理超过天数的记录（0 为不按时间清理）", "0")?;
    app.press("保存清理设置")?;
    wait_store(app, "valid per-type draft persisted", || {
        let p = fixture.store.preferences().unwrap();
        p.cleanup.mode == yank_core::CleanupMode::PerType
            && p.cleanup.per_type.get("color") == Some(&15)
            && p.retention_days == 0
    })?;
    assert_eq!(
        fixture.store.stats().unwrap().clips,
        2,
        "disabled cleanup never deletes"
    );
    Ok(())
}
