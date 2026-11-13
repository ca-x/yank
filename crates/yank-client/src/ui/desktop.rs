//! Native desktop chrome, floating cards, tray and global shortcuts.
use super::{
    Ui,
    controls::{action, hint, surface},
};
use crate::worker::Command;
use nsh_base::theme;
use nsh_core::{
    desktop::{WindowAttributes, WindowFrame, WindowKind, WindowLevel},
    element::desktop as native,
    prelude::*,
};
use yank_core::Clip;

pub(super) fn install(ui: Ui) {
    if !is_desktop() {
        return;
    }
    let state = ui.clone();
    let mut main_attributes = WindowAttributes::default();
    main_attributes.close_on_press = Some(false);
    native::main_window().set(main_attributes);
    native::main_window().on_close_press(move |_| native::main_window().hide());
    let open = ui.clone();
    let pause = ui.clone();
    let queue = ui.clone();
    let settings = ui.clone();
    let floats = ui.clone();
    native::set_tray(Some(
        native::tray()
            .tooltip("Yank · 剪贴板历史")
            .on_click(move |event| {
                open.tray_anchor.set(event.bounds);
                show(open.clone());
            })
            .menu([
                native::MenuItem::new("打开剪贴板").on_select(move || show(state.clone())),
                native::MenuItem::new("暂停 / 继续记录").on_select(move || {
                    let mut p = pause.prefs.get();
                    p.capture = !p.capture;
                    pause.save(p);
                }),
                native::MenuItem::new("粘贴队列下一条").on_select(move || next(queue.clone())),
                native::MenuItem::new("关闭全部悬浮卡片").on_select(move || {
                    let windows: Vec<_> = floats
                        .windows
                        .borrow_mut()
                        .drain()
                        .map(|(_, window)| window)
                        .collect();
                    for window in windows {
                        window.close();
                    }
                    floats.floating.set(Vec::new());
                }),
                native::MenuItem::new("设置").on_select(move || {
                    settings.page.set("settings".into());
                    native::main_window().focus();
                }),
                native::MenuItem::new("退出 Yank").on_select(native::quit),
            ]),
    ));
    let state = ui.clone();
    effect(move || {
        let p = state.prefs.get();
        native::unregister_all_global_shortcuts();
        let show_ui = state.clone();
        bind(&p.hotkey_show, move || show(show_ui.clone()), state.clone());
        let pause = state.clone();
        bind(
            &p.hotkey_pause,
            move || {
                let mut p = pause.prefs.get();
                p.capture = !p.capture;
                pause.save(p);
            },
            state.clone(),
        );
        let queue = state.clone();
        bind(&p.hotkey_next, move || next(queue.clone()), state.clone());
        let float = state.clone();
        bind(
            &p.hotkey_float,
            move || {
                float.open_float();
            },
            state.clone(),
        );
    });
    let preview = ui.clone();
    on_key_with(
        "space",
        KeyOptions {
            when: nsh_core::keymap::When::NotEditing,
            ..Default::default()
        },
        move |_| {
            if preview.page.get() == "history" {
                preview.page.set("preview".into());
            }
        },
    );
}
fn bind(key: &str, run: impl Fn() + 'static, ui: Ui) {
    if key.trim().is_empty() {
        return;
    }
    let answer = native::register_global_shortcut(key, run);
    let name = key.to_owned();
    spawn(async move {
        if !answer.await {
            ui.notice.set(format!(
                "全局快捷键 {name} 未获系统授权或已被占用，请在设置中更改。"
            ));
        }
    });
}
pub(super) fn show(ui: Ui) {
    let p = ui.prefs.get();
    let anchor = if p.panel_position == "tray" {
        ui.tray_anchor.get()
    } else {
        None
    };
    let screens = native::displays();
    let screen = anchor
        .and_then(|anchor| {
            screens
                .iter()
                .find(|screen| screen.bounds.contains(&anchor.center()))
        })
        .or_else(|| screens.iter().find(|screen| screen.primary))
        .or_else(|| screens.first());
    if let Some(screen) = screen {
        native::main_window().set_bounds(panel_frame(
            screen.visible_bounds,
            p.panel_size,
            &p.panel_position,
            anchor,
        ));
    }
    ui.page.set("history".into());
    native::main_window().focus();
}
// Tray bounds and display bounds use the same screen coordinates in NSH.
fn panel_frame(
    area: Bounds<Points>,
    panel_size: u32,
    position: &str,
    anchor: Option<Bounds<Points>>,
) -> WindowFrame {
    let width = (600.0 + (panel_size.min(5) as f32) * 120.0).min(area.size.width.0.max(0.0));
    let height = 680.0f32.min(area.size.height.0.max(0.0));
    let left = area.origin.x.0;
    let top = area.origin.y.0;
    let right = left + area.size.width.0 - width;
    let bottom = top + area.size.height.0 - height;
    let cx = (left + right) / 2.0;
    let cy = (top + bottom) / 2.0;
    let (x, y) = match (position, anchor) {
        ("tray", Some(anchor)) => {
            let below = anchor.origin.y.0 + anchor.size.height.0 + 8.0;
            let y = if below + height <= top + area.size.height.0 {
                below
            } else {
                anchor.origin.y.0 - height - 8.0
            };
            (anchor.center().x.0 - width / 2.0, y)
        }
        ("top", _) => (cx, top),
        ("bottom", _) => (cx, bottom),
        ("left", _) => (left, cy),
        ("right", _) => (right, cy),
        _ => (cx, cy),
    };
    WindowFrame {
        x: Some(px(x.clamp(left, right.max(left)))),
        y: Some(px(y.clamp(top, bottom.max(top)))),
        width: Some(px(width)),
        height: Some(px(height)),
        animated: Some(false),
    }
}

pub(super) fn next(ui: Ui) {
    let clip = ui.queue.with(|q| q.first().cloned());
    if let Some(clip) = clip {
        ui.queue.update(|q| {
            q.remove(0);
        });
        native::main_window().hide();
        ui.send(Command::Paste(clip));
    } else {
        ui.notice.set("粘贴队列已经用完。".into());
    }
}
pub(super) fn floating(ui: Ui, clip: Clip) {
    if !is_desktop() {
        ui.page.set("preview".into());
        return;
    }
    let id = format!("floating:{}", clip.id);
    if let Some(window) = ui.windows.borrow().get(&id).cloned() {
        window.focus();
        return;
    }
    if ui.floating.with(Vec::len) >= 8 {
        ui.notice.set("最多同时打开 8 张悬浮卡片。".into());
        return;
    }
    let close_ui = ui.clone();
    let close_id = id.clone();
    let text = clip.text.clone();
    let copy = clip.clone();
    let label = clip.description.clone();
    let content_ui = ui.clone();
    let mut attributes = WindowAttributes::default();
    attributes.level = Some(WindowLevel::Floating);
    attributes.taskbar = Some(false);
    let card = native::window()
        .id(&id)
        .kind(WindowKind::Panel)
        .title(label)
        .default_size(size(px(380.0), px(300.0)))
        .attributes(attributes)
        .on_open_change(move |event| {
            if event.open {
                return;
            }
            close_ui.windows.borrow_mut().remove(&close_id);
            close_ui
                .floating
                .update(|ids| ids.retain(|v| v != &close_id))
        })
        .content(move || {
            let u = content_ui.clone();
            let c = copy.clone();
            surface()
                .flex_1()
                .child(hint("悬浮预览 · 已保存内容 · 保持置顶"))
                .child(
                    scroll().flex_1().child(
                        super::safe_text_view()
                            .markdown(Some(SharedString::from(text.clone())))
                            .selectable(true)
                            .text_color(move || theme().foreground),
                    ),
                )
                .child(action("复制已保存内容", true, move || {
                    u.copy_clip(c.clone())
                }))
                .child(action("关闭", false, move || {
                    native::current_window().close()
                }))
        });
    if let Some(window) = native::open_window(card) {
        ui.windows.borrow_mut().insert(id.clone(), window);
        ui.floating.update(|ids| ids.push(id));
    } else {
        ui.notice.set("当前平台未允许创建悬浮窗口。".into());
    }
}
