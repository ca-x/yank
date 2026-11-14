//! Native desktop chrome, floating cards, tray and global shortcuts.
use super::{
    Ui,
    controls::{action, hint, surface},
};
use crate::worker::Command;
use base64::Engine;
use nsh_base::theme;
use nsh_core::{
    desktop::{WindowAttributes, WindowFrame, WindowKind, WindowLevel},
    element::desktop as native,
    prelude::*,
};
use yank_core::{Clip, CloseBehavior, WindowStyle};

pub(super) fn install(ui: Ui) {
    if !is_desktop() {
        return;
    }
    let chrome = ui.clone();
    effect(move || {
        let mut attributes = WindowAttributes::default();
        attributes.close_on_press = Some(false);
        attributes.frame = Some(match chrome.prefs.get().window_style {
            WindowStyle::System => nsh_core::desktop::WindowFrameStyle::Default,
            WindowStyle::TrafficLights => nsh_core::desktop::WindowFrameStyle::None,
        });
        native::main_window().set(attributes);
    });
    let close = ui.clone();
    native::main_window().on_close_press(move |event| {
        if event == nsh_core::desktop::CloseReason::System {
            native::quit();
        } else {
            request_close(close.clone());
        }
    });
    let saved_close = ui.clone();
    effect(move || {
        if let Some(behavior) = saved_close.close_after_save.get()
            && let Some(snapshot) = saved_close.snapshot.get()
        {
            if snapshot.prefs.close_behavior == behavior {
                saved_close.close_after_save.set(None);
                finish_close(behavior);
            } else if !snapshot.error.is_empty() {
                saved_close.close_after_save.set(None);
            }
        }
    });
    let tray_ui = ui.clone();
    effect(move || {
        crate::l10n::language();
        let ui = tray_ui.clone();
        let state = ui.clone();
        let open = ui.clone();
        let pause = ui.clone();
        let queue = ui.clone();
        let settings = ui.clone();
        let floats = ui.clone();
        native::set_tray(Some(
            native::tray()
                .icon(format!(
                    "data:image/png;base64,{}",
                    base64::engine::general_purpose::STANDARD
                        .encode(include_bytes!("../../assets/yank-icon-64.png"))
                ))
                .tooltip(crate::l10n::tr("Yank · 剪贴板历史"))
                .on_click(move |event| {
                    open.tray_anchor.set(event.bounds);
                    show(open.clone());
                })
                .menu([
                    native::MenuItem::new(crate::l10n::tr("打开剪贴板"))
                        .on_select(move || show(state.clone())),
                    native::MenuItem::new(crate::l10n::tr("暂停 / 继续记录")).on_select(
                        move || {
                            let mut p = pause.prefs.get();
                            p.capture = !p.capture;
                            pause.save(p);
                        },
                    ),
                    native::MenuItem::new(crate::l10n::tr("粘贴队列下一条"))
                        .on_select(move || next(queue.clone())),
                    native::MenuItem::new(crate::l10n::tr("关闭全部悬浮卡片")).on_select(
                        move || {
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
                        },
                    ),
                    native::MenuItem::new(crate::l10n::tr("设置")).on_select(move || {
                        settings.page.set("settings".into());
                        native::main_window().focus();
                    }),
                    native::MenuItem::new(crate::l10n::tr("退出 Yank")).on_select(native::quit),
                ]),
        ));
    });
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

fn finish_close(behavior: CloseBehavior) {
    match behavior {
        CloseBehavior::Minimize => native::main_window().minimize(),
        CloseBehavior::Quit => native::quit(),
        CloseBehavior::Ask => {}
    }
}

pub(super) fn request_close(ui: Ui) {
    if ui.close_prompting.get() || ui.close_after_save.get().is_some() {
        return;
    }
    match ui.prefs.get().close_behavior {
        CloseBehavior::Ask => {
            ui.close_prompting.set(true);
            let answer = nsh_base::alert_dialog()
                .title(crate::l10n::tr("关闭窗口时怎么做？"))
                .description(crate::l10n::tr(
                    "最小化后继续记录与同步；退出会停止后台任务。选择会被记住，也可在设置中修改。",
                ))
                .cancel("cancel", crate::l10n::tr("取消"))
                .action("minimize", crate::l10n::tr("最小化并继续运行"))
                .destructive("quit", crate::l10n::tr("退出程序"))
                .show();
            spawn(async move {
                let behavior = match answer
                    .await
                    .ok()
                    .and_then(|answer| answer.action)
                    .as_deref()
                {
                    Some("minimize") => Some(CloseBehavior::Minimize),
                    Some("quit") => Some(CloseBehavior::Quit),
                    _ => None,
                };
                ui.close_prompting.set(false);
                if let Some(behavior) = behavior {
                    let mut prefs = ui.prefs.get();
                    prefs.close_behavior = behavior;
                    ui.save(prefs);
                    ui.close_after_save.set(Some(behavior));
                }
            });
        }
        behavior => finish_close(behavior),
    }
}

pub(super) fn title_bar(ui: Ui) -> impl IntoElement {
    let visible = ui.clone();
    when(
        move || visible.prefs.get().window_style == WindowStyle::TrafficLights,
        move || {
            use nsh_core::desktop::WindowControl;
            let mut controls = view().flex_row().items_center().gap_1().window_drag(false);
            for (control, label, mark, token) in [
                (WindowControl::Close, "关闭窗口", "×", "window-close"),
                (
                    WindowControl::Minimize,
                    "最小化窗口",
                    "−",
                    "window-minimize",
                ),
                (
                    WindowControl::Maximize,
                    "最大化或还原窗口",
                    "+",
                    "window-maximize",
                ),
            ] {
                let close = ui.clone();
                controls = controls.child(
                    nsh_base::button()
                        .on_press(move || match control {
                            WindowControl::Close => request_close(close.clone()),
                            WindowControl::Minimize => native::main_window().minimize(),
                            WindowControl::Maximize => {
                                let maximized =
                                    native::main_window().state().is_some_and(|state| {
                                        state.mode == nsh_core::desktop::WindowMode::Maximized
                                    });
                                native::main_window().set_mode(if maximized {
                                    nsh_core::desktop::WindowMode::Normal
                                } else {
                                    nsh_core::desktop::WindowMode::Maximized
                                });
                            }
                        })
                        .accessibility_label(move || {
                            Some(SharedString::from(crate::l10n::tr(label)))
                        })
                        .w(px(36.0))
                        .h(px(36.0))
                        .items_center()
                        .justify_center()
                        .rounded(px(8.0))
                        .hover(|style| style.bg(theme().accent))
                        .active(|style| style.bg(theme().muted))
                        .focus(|style| style.outline(px(2.0)).outline_color(theme().ring))
                        .child(
                            view()
                                .w(px(18.0))
                                .h(px(18.0))
                                .rounded(px(9.0))
                                .items_center()
                                .justify_center()
                                .bg(move || theme().color(token).unwrap_or(theme().primary))
                                .child(super::text(mark).text_xs().text_color(move || {
                                    theme().color("window-symbol").unwrap_or(theme().foreground)
                                })),
                        ),
                );
            }
            view()
                .flex_row()
                .items_center()
                .gap_3()
                .px_3()
                .h(px(44.0))
                .bg(move || theme().secondary)
                .border_b_1()
                .border_color(move || theme().border)
                .window_drag(true)
                .child(controls)
                .child(
                    super::text("yank")
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(view().flex_1())
        },
    )
}

pub(super) fn brand(ui: Ui) -> impl IntoElement {
    let lights = ui.clone();
    let system = ui.clone();
    view()
        .gap_2()
        .window_drag(true)
        .child(when(
            move || lights.prefs.get().window_style == WindowStyle::TrafficLights,
            move || title_bar(ui.clone()),
        ))
        .child(when(
            move || system.prefs.get().window_style != WindowStyle::TrafficLights,
            || {
                super::text("yank")
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().primary)
            },
        ))
}
fn bind(key: &str, run: impl Fn() + 'static, ui: Ui) {
    if key.trim().is_empty() {
        return;
    }
    let answer = native::register_global_shortcut(key, run);
    let name = key.to_owned();
    spawn(async move {
        if !answer.await {
            ui.alert.set(crate::l10n::message(
                "全局快捷键 {name} 未获系统授权或已被占用，请在设置中更改。",
                &[("name", super::platform_shortcut(&name))],
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
    native::main_window().restore();
    native::main_window().focus();
    ui.search_field.focus();
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

pub(super) fn paste_selected(ui: Ui, index: Option<usize>) {
    if !is_desktop() {
        return;
    }
    let id = ui.snapshot.with(|snapshot| {
        let snapshot = snapshot.as_ref()?;
        if snapshot.query != ui.query.get() || snapshot.filter != ui.filter.get() {
            return None;
        }
        let selected = ui.selected.get();
        paste_target(&snapshot.clips, &selected, index)
    });
    if let Some(id) = id {
        ui.selected.set(id.clone());
        paste_record(ui, id);
    } else {
        ui.notice
            .set("没有可粘贴的记录，请等待搜索完成或重新选择。".into());
    }
}

pub(super) fn paste_record(ui: Ui, id: String) {
    if is_desktop() {
        native::main_window().hide();
        ui.send(Command::Paste(id));
    }
}

fn paste_target(clips: &[Clip], selected: &str, index: Option<usize>) -> Option<String> {
    let clip = match index {
        Some(index) => clips.get(index),
        None if selected.is_empty() => clips.first(),
        None => clips.iter().find(|clip| clip.id == selected),
    };
    clip.map(|clip| clip.id.clone())
}

pub(super) fn next(ui: Ui) {
    let clip = ui.queue.with(|q| q.first().cloned());
    if let Some(clip) = clip {
        ui.queue_undo.set(Some(clip.clone()));
        ui.queue.update(|q| {
            q.remove(0);
        });
        native::main_window().hide();
        ui.send(Command::Paste(clip.id));
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

#[cfg(test)]
mod quick_paste_tests {
    use super::*;

    #[test]
    fn deleted_selection_does_not_silently_paste_another_record() {
        let clips = [Clip::text("test", "first"), Clip::text("test", "second")];
        assert_eq!(paste_target(&clips, "", None), Some(clips[0].id.clone()));
        assert_eq!(
            paste_target(&clips, &clips[1].id, None),
            Some(clips[1].id.clone())
        );
        assert_eq!(paste_target(&clips, "deleted", None), None);
        assert_eq!(paste_target(&clips, "", Some(9)), None);
        assert_eq!(paste_target(&[], "", None), None);
    }
}
