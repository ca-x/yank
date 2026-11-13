use yank_core::{Clip, Preferences, Store, WindowStyle};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: seed_demo <empty-demo-database>")?;
    let store = Store::open(path)?;
    if store.stats()?.clips > 0 {
        return Err("refusing to replace an existing clipboard library".into());
    }
    store.save_preferences(&Preferences {
        capture: false,
        apply_remote: false,
        backup_daily: false,
        mode: "direct".into(),
        window_style: WindowStyle::TrafficLights,
        device_name: "Yank 演示设备".into(),
        ..Default::default()
    })?;
    for (kind, title, content, pinned) in [
        (
            "text",
            "周五开发笔记",
            "先把内容找回来，再接着手上的工作。\n\n今天的任务：完善历史浏览、检查同步、整理发布记录。",
            false,
        ),
        (
            "code",
            "SQLite · 稳定游标分页",
            "SELECT id, body FROM clips\nWHERE deleted = 0 AND recency < :cursor\nORDER BY recency DESC\nLIMIT 100;",
            false,
        ),
        (
            "link",
            "Rust 标准库文档",
            "https://doc.rust-lang.org/std/",
            false,
        ),
        (
            "text",
            "会议纪要 · 多端同步",
            "清理策略留在本机，删除事实参与同步。\n离线编辑保留冲突版本，恢复时生成新的因果版本。",
            false,
        ),
        ("color", "品牌色 · 松绿色", "#176b54", false),
        (
            "text",
            "发布前检查",
            "格式检查 → 单元测试 → 本地界面验证 → 截图 → 打包",
            true,
        ),
        (
            "text",
            "剪贴板，也可以井然有序",
            "Ctrl + Shift + V 打开历史\n↑ ↓ 选择 · Enter 粘贴\n搜索、收藏和列表，让常用内容随手可得。",
            false,
        ),
    ] {
        let mut clip = Clip::text("demo", content);
        clip.description = title.into();
        clip.kind = kind.into();
        clip.pinned = pinned;
        clip.collection = if pinned { "工作资料" } else { "" }.into();
        store.save_local(clip, "demo")?;
    }
    println!("Seeded seven demo records; capture and remote clipboard writes are disabled.");
    Ok(())
}
