//! Responsive, content-first native clipboard workspace.
mod controls;
mod desktop;
use crate::{
    network::{Association, Decision},
    worker::{CaptureSource, Command, Snapshot, Worker},
};
use controls::{action, field, hint, surface, title, toggle};
use nsh_base::{alert_dialog, theme};
use nsh_core::{platform::Appearance, prelude::*};
use std::{collections::HashMap, rc::Rc, sync::atomic::Ordering, time::Duration};
use yank_core::{Clip, ClipFormat, Preferences};

#[derive(Clone, Debug, PartialEq)]
struct Draft {
    text: String,
    collection: String,
}
impl From<&Clip> for Draft {
    fn from(clip: &Clip) -> Self {
        Self {
            text: clip.text.clone(),
            collection: clip.collection.clone(),
        }
    }
}

// Unknown/native formats may carry binary data even when the clip has a text fallback.
fn text_editable(clip: &Clip) -> bool {
    !matches!(clip.kind.as_str(), "image" | "files")
        && clip.formats.iter().all(|f| {
            f.mime
                .as_deref()
                .is_some_and(|mime| mime.starts_with("text/"))
        })
}

fn reduce_permissions(shared: &mut Preferences, proposed: &Preferences) {
    if !proposed.upload {
        shared.upload = false;
    }
    if proposed.token.is_empty() {
        shared.detach_account();
    }
    if !proposed.apply_remote {
        shared.apply_remote = false;
    }
}

#[derive(Clone)]
struct Ui {
    worker: Rc<Worker>,
    busy: Signal<String>,
    snapshot: Signal<Option<Snapshot>>,
    page: Signal<String>,
    query: Signal<String>,
    search_field: ElementRef,
    history_list: ElementRef,
    filter: Signal<String>,
    selected: Signal<String>,
    prefs: Signal<Preferences>,
    drafts: Signal<HashMap<String, Draft>>,
    preference_drafts: Signal<HashMap<&'static str, String>>,
    pending_float: Signal<Option<String>>,
    tray_anchor: Signal<Option<Bounds<Points>>>,
    picking: Signal<bool>,
    collecting: Signal<bool>,
    recovery_pending: Signal<bool>,
    recovery_path: Signal<Option<std::path::PathBuf>>,
    queue: Signal<Vec<Clip>>,
    floating: Signal<Vec<String>>,
    windows: Rc<
        std::cell::RefCell<
            std::collections::HashMap<String, nsh_core::element::desktop::WindowRef>,
        >,
    >,
    smart_name: Signal<String>,
    smart_days: Signal<u32>,
    notice: Signal<String>,
    server: Signal<String>,
    username: Signal<String>,
    password: Signal<String>,
    code: Signal<String>,
    limit: Signal<u32>,
    prompting: Signal<bool>,
}
impl Ui {
    fn new(worker: Rc<Worker>) -> Self {
        Self {
            worker,
            busy: signal(String::new()),
            snapshot: signal(None),
            page: signal("history".into()),
            query: signal(String::new()),
            search_field: element_ref(),
            history_list: element_ref(),
            filter: signal("all".into()),
            selected: signal(String::new()),
            prefs: signal(Preferences::default()),
            drafts: signal(HashMap::new()),
            preference_drafts: signal(HashMap::new()),
            pending_float: signal(None),
            tray_anchor: signal(None),
            picking: signal(false),
            collecting: signal(false),
            recovery_pending: signal(false),
            recovery_path: signal(None),
            queue: signal(Vec::new()),
            floating: signal(Vec::new()),
            windows: Rc::default(),
            smart_name: signal(String::new()),
            smart_days: signal(0),
            notice: signal(String::new()),
            server: signal(String::new()),
            username: signal(String::new()),
            password: signal(String::new()),
            code: signal(String::new()),
            limit: signal(100),
            prompting: signal(false),
        }
    }

    fn send(&self, mut command: Command) {
        if matches!(&command, Command::Logout) {
            let mut p = self.prefs.get();
            p.detach_account();
            self.revoke(&p, true);
            self.prefs.set(p);
        }
        let task = match &command {
            Command::Login(..) => Some("正在登录…"),
            Command::Export(..) => Some("正在导出…"),
            Command::Import(..) => Some("正在导入…"),
            Command::SendDitto(..) => Some("正在发送到 Ditto…"),
            _ => None,
        };
        if let Some(task) = task
            && let Ok(mut busy) = self.worker.shared.busy.lock()
        {
            if busy.is_some() {
                self.notice.set("请等待当前任务完成".into());
                return;
            }
            *busy = Some(task.into());
            self.busy.set(task.into());
        }

        if matches!(&command, Command::Login(..) | Command::Save(..)) {
            if let Ok(_permissions) = self.worker.shared.prefs.lock() {
                match &mut command {
                    Command::Login(_, _, _, revision) => {
                        *revision = self
                            .worker
                            .shared
                            .auth_revision
                            .fetch_add(1, Ordering::SeqCst)
                            + 1
                    }
                    Command::Save(_, revision) => {
                        *revision = self.worker.shared.auth_revision.load(Ordering::SeqCst)
                    }
                    _ => {}
                }
            } else {
                if task.is_some() {
                    self.busy.set(String::new());
                    if let Ok(mut busy) = self.worker.shared.busy.lock() {
                        *busy = None;
                    }
                }
                self.notice.set("设置不可用，请重启应用".into());
                return;
            }
        }
        if let Command::PairCode(_, _, epoch) = &mut command {
            *epoch = self.worker.shared.pair_epoch.load(Ordering::SeqCst);
        }
        if let Command::Unpair(id) = &command {
            self.worker.shared.revoke_peer(id);
        }
        if self.worker.commands.send(command).is_err() {
            self.busy.set(String::new());
            if let Ok(mut busy) = self.worker.shared.busy.lock() {
                *busy = None;
            }
            self.notice.set("后台服务已停止，请重启应用".into());
        }
    }
    fn selected(&self) -> Option<Clip> {
        let id = self.selected.get();
        self.snapshot.with(|s| {
            s.as_ref()
                .and_then(|s| s.selected.as_ref().filter(|c| c.id == id).cloned())
        })
    }
    fn choose(&self, clip: Clip) {
        self.pending_float.set(None);
        let hydrated = self
            .selected()
            .is_some_and(|selected| selected.id == clip.id);
        self.selected.set(clip.id.clone());
        if !hydrated {
            self.send(Command::Select(clip.id));
        }

        if window().width.0 < 850.0 || !is_desktop() {
            self.page.set("detail".into());
        }
    }
    fn copy(&self) {
        if let Some(clip) = self.selected() {
            self.copy_clip(clip);
        }
    }
    fn copy_clip(&self, clip: Clip) {
        if is_desktop() {
            self.send(Command::Copy(clip, false));
        } else if !clip.text.is_empty() {
            set_clipboard(clip.text);
            self.notice.set("已复制，可以接着使用了".into());
        } else {
            self.notice.set("此平台不支持写入这种剪贴板格式".into());
        }
    }
    fn revoke(&self, prefs: &Preferences, logout: bool) {
        if let Ok(mut shared) = self.worker.shared.prefs.lock() {
            let pending_login = prefs.token.is_empty()
                && self
                    .worker
                    .shared
                    .busy
                    .lock()
                    .is_ok_and(|busy| busy.as_deref() == Some("正在登录…"));
            if logout
                || (shared.upload && !prefs.upload)
                || shared.token != prefs.token
                || shared.server_url != prefs.server_url
                || (shared.apply_remote && !prefs.apply_remote)
                || pending_login
            {
                self.worker
                    .shared
                    .auth_revision
                    .fetch_add(1, Ordering::SeqCst);
            }
            reduce_permissions(&mut shared, prefs);
        }
    }
    fn save(&self, prefs: Preferences) {
        self.revoke(&prefs, false);
        let mut displayed = prefs.clone();
        // Enabling uploads becomes visible only after the worker validates and persists it.
        displayed.upload &= self.worker.shared.prefs.lock().is_ok_and(|p| p.upload);
        self.prefs.set(displayed);
        self.send(Command::Save(prefs, 0));
    }
    fn draft(&self) -> Option<Draft> {
        let clip = self.selected()?;
        Some(
            self.drafts
                .with(|drafts| drafts.get(&clip.id).cloned())
                .unwrap_or_else(|| Draft::from(&clip)),
        )
    }
    fn edit_draft(&self, edit: impl FnOnce(&mut Draft)) {
        if let Some(clip) = self.selected() {
            self.drafts.update(|drafts| {
                edit(
                    drafts
                        .entry(clip.id.clone())
                        .or_insert_with(|| Draft::from(&clip)),
                )
            });
        }
    }
    fn dirty(&self) -> bool {
        self.selected()
            .zip(self.draft())
            .is_some_and(|(clip, draft)| draft != Draft::from(&clip))
    }
    fn pending(&self) -> bool {
        !self.busy.get().is_empty() || self.picking.get()
    }
    fn feedback(&self) -> String {
        if self.pending() {
            return if self.picking.get() {
                "请选择文件…".into()
            } else {
                self.busy.get()
            };
        }
        let error = self
            .snapshot
            .with(|s| s.as_ref().map(|s| s.error.clone()).unwrap_or_default());
        if !error.is_empty() {
            format!("操作失败：{error}")
        } else {
            self.notice.get()
        }
    }
    fn back(&self) -> bool {
        if keyboard_height().0 > 0.0 {
            dismiss_keyboard();
            return true;
        }
        let next = match self.page.get().as_str() {
            "preview"
                if !self.selected.get().is_empty()
                    && (window().width.0 < 850.0 || !is_desktop()) =>
            {
                "detail"
            }
            "history" => return false,
            _ => "history",
        };
        self.page.set(next.into());
        true
    }
    fn open_float(&self) {
        if let Some(clip) = self.selected() {
            desktop::floating(self.clone(), clip);
        } else if let Some(id) = if self.selected.get().is_empty() {
            self.snapshot.with(|s| {
                s.as_ref()
                    .and_then(|s| s.clips.first().map(|c| c.id.clone()))
            })
        } else {
            Some(self.selected.get())
        } {
            self.selected.set(id.clone());
            self.pending_float.set(Some(id.clone()));
            self.send(Command::Select(id));
        }
    }
}

pub fn app() -> impl IntoElement {
    if let Ok(tokens) = nsh_base::tokens_of(Some(include_str!("theme.json"))) {
        set_tokens(tokens);
    }
    let native_dir = data_dir();
    let real = native_dir.is_some();
    let directory = std::env::var_os("YANK_DATA_DIR")
        .map(std::path::PathBuf::from)
        .or(native_dir)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("yank-headless-{}", yank_core::new_id()))
        });
    let worker = match Worker::start(directory, real) {
        Ok(worker) => Rc::new(worker),
        Err(error) => {
            return view()
                .p_6()
                .child(text(format!("无法打开本地资料库：{error}")))
                .into_element();
        }
    };
    let ui = Ui::new(worker);
    let appearance = ui.clone();
    effect(move || {
        set_color_scheme(match appearance.prefs.get().theme.as_str() {
            "dark" => Appearance::Dark,
            "light" => Appearance::Light,
            _ => Appearance::System,
        })
    });
    desktop::install(ui.clone());
    let back = ui.clone();
    on_back_press(move || back.back());
    let state = ui.clone();
    cleanup(move || state.worker.shared.stop.store(true, Ordering::Relaxed));
    let state = ui.clone();
    let startup_done = std::cell::Cell::new(false);
    set_interval(Duration::from_millis(80), move || {
        state
            .recovery_pending
            .set(state.worker.shared.recovery_pending.load(Ordering::Relaxed));
        let captured = state
            .worker
            .shared
            .collected
            .lock()
            .map(|mut clips| std::mem::take(&mut *clips))
            .unwrap_or_default();
        if !captured.is_empty() {
            state.queue.update(|queue| {
                for clip in captured {
                    if queue.len() >= 1000 {
                        break;
                    }
                    if !queue.iter().any(|c| c.id == clip.id) {
                        queue.push(clip);
                    }
                }
            });
        }
        state.busy.set(
            state
                .worker
                .shared
                .busy
                .lock()
                .ok()
                .and_then(|s| s.clone())
                .unwrap_or_default(),
        );
        while let Ok(snapshot) = state.worker.snapshots.try_recv() {
            if !startup_done.replace(true)
                && is_desktop()
                && (snapshot.prefs.silent_start
                    || std::env::args().any(|arg| arg == "--background"))
            {
                nsh_core::element::desktop::main_window().hide();
            }
            if state.snapshot.with(Option::is_none) {
                state.server.set(snapshot.prefs.server_url.clone());
                state.username.set(snapshot.prefs.username.clone());
            }
            if state.prefs.get().token != snapshot.prefs.token && !snapshot.prefs.token.is_empty() {
                state.password.set(String::new());
            }
            state.prefs.set(snapshot.prefs.clone());
            if snapshot.query == state.query.get() && snapshot.filter == state.filter.get() {
                if let Some(clip) = &snapshot.selected {
                    state.drafts.update(|drafts| {
                        if drafts
                            .get(&clip.id)
                            .is_some_and(|draft| *draft == Draft::from(clip))
                        {
                            drafts.remove(&clip.id);
                        }
                    });
                }
                state.snapshot.set(Some(snapshot));
                if let Some(id) = state.pending_float.get()
                    && let Some(clip) = state.selected().filter(|clip| clip.id == id)
                {
                    state.pending_float.set(None);
                    desktop::floating(state.clone(), clip);
                }
            }
        }
        if state.recovery_pending.get() && !state.prompting.get() && !state.pending() {
            let path = state
                .worker
                .shared
                .recovery_backup
                .lock()
                .ok()
                .and_then(|mut path| path.take());
            if let Some(path) = path {
                state.recovery_path.set(Some(path.clone()));
                state.prompting.set(true);
                let ui = state.clone();
                let answer = alert_dialog().title("本地历史文件不存在，恢复自动备份？")
                    .description("仅恢复剪贴板历史，不包含账号或登录凭据。恢复失败后可以在设置中重试；等待恢复期间暂停自动采集和自动备份，保护原备份。")
                    .cancel("empty", "暂不恢复，使用空资料库").action("restore", "恢复历史").show();
                spawn(async move {
                    if answer
                        .await
                        .is_ok_and(|a| a.action.as_deref() == Some("restore"))
                    {
                        ui.send(Command::Import(path));
                    } else {
                        ui.worker
                            .shared
                            .recovery_pending
                            .store(false, Ordering::Relaxed);
                        ui.recovery_pending.set(false);
                    }
                    ui.prompting.set(false);
                });
            }
        }
        if !state.prompting.get() {
            let pending = state
                .worker
                .shared
                .prompts
                .lock()
                .ok()
                .and_then(|p| p.first().map(|p| (p.peer.clone(), p.account.clone())));
            if let Some((peer, invitation)) = pending {
                state.prompting.set(true);
                let ui = state.clone();
                let mut dialog = alert_dialog()
                    .title(format!("与 {} 配对？", peer.name))
                    .description(format!(
                        "设备码 {}。确认后，这台设备可以收发你的剪贴板记录。",
                        peer.code
                    ))
                    .cancel("reject", "拒绝")
                    .action("pair", "仅设备互通");
                if invitation.is_some() {
                    dialog = dialog.action("account", "配对并选择登录关联账号");
                } else if !state.prefs.get().token.is_empty() {
                    dialog = dialog.action("account", "互通并邀请关联账号");
                }
                let answer = dialog.show();
                spawn(async move {
                    let choice = answer.await.ok().and_then(|a| a.action);
                    let prefs = ui.prefs.get();
                    let account = if choice.as_deref() == Some("account") {
                        if let Some(invitation) = invitation {
                            if let Ok(mut value) = ui.worker.shared.association.lock() {
                                *value = Some(invitation);
                            }
                            None
                        } else {
                            Some(Association {
                                server_url: prefs.server_url,
                                username: prefs.username,
                            })
                        }
                    } else {
                        None
                    };
                    ui.worker.shared.decide(
                        &peer.id,
                        Decision {
                            accept: matches!(choice.as_deref(), Some("pair" | "account")),
                            account,
                        },
                    );
                    ui.prompting.set(false);
                });
            }
        }
        let association = state
            .worker
            .shared
            .association
            .lock()
            .ok()
            .and_then(|mut a| a.take());
        if let Some(a) = association {
            state.server.set(a.server_url);
            state.username.set(a.username);
            state.page.set("settings".into());
            state
                .notice
                .set("对方邀请你关联账号；可以登录，也可以保持仅设备互通。".into());
        }
    });
    let state = ui.clone();
    on_key("mod+f", move |_| {
        state.page.set("history".into());
        state.search_field.focus();
    });
    let state = ui.clone();
    on_key_with(
        "escape",
        KeyOptions {
            when: nsh_core::keymap::When::Always,
            ..Default::default()
        },
        move |_| {
            if !state.back() && is_desktop() {
                nsh_core::element::desktop::main_window().hide();
            }
        },
    );
    let state = ui.clone();
    on_key("ctrl+enter", move |_| {
        if let Some(clip) = state.selected() {
            if is_desktop() {
                state.send(Command::Copy(clip, true));
            } else {
                set_clipboard(clip.text);
            }
        }
    });
    let desktop = computed(|| window().width.0 >= 850.0 && is_desktop());
    let top = ui.clone();
    let side = ui.clone();
    let content = ui.clone();
    let foot = ui.clone();
    view()
        .flex_1()
        .p_safe_area()
        .pb(move || px(safe_area().bottom.0.max(keyboard_height().0)))
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(
            view()
                .flex_row()
                .flex_1()
                .min_h_0()
                .child(when(move || desktop.get(), move || sidebar(side.clone())))
                .child(
                    view()
                        .flex_1()
                        .min_w_0()
                        .child(header(top))
                        .child(when(move || !desktop.get(), {
                            let ui = ui.clone();
                            move || mobile_nav(ui.clone())
                        }))
                        .child(dynamic(move || match content.page.get().as_str() {
                            "devices" => devices(content.clone()).into_element(),
                            "settings" => settings(content.clone()).into_element(),
                            "detail" => details(content.clone()).into_element(),
                            "queue" => queue(content.clone()).into_element(),
                            "preview" => preview(content.clone()).into_element(),
                            "smart" => smart_editor(content.clone()).into_element(),
                            "lists" => lists(content.clone()).into_element(),
                            _ => history(content.clone(), desktop).into_element(),
                        })),
                ),
        )
        .child(
            view()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(move || theme().border)
                .child(
                    text(move || {
                        let error = foot
                            .snapshot
                            .with(|s| s.as_ref().map(|s| s.error.clone()).unwrap_or_default());
                        if !error.is_empty() {
                            return format!("操作失败：{error}");
                        }
                        let notice = foot.notice.get();
                        if !notice.is_empty() {
                            notice
                        } else {
                            foot.worker
                                .shared
                                .status
                                .lock()
                                .map(|s| s.clone())
                                .unwrap_or_default()
                        }
                    })
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .into_element()
}
fn navigate(ui: &Ui, delta: isize) {
    if ui.page.get() != "history" {
        return;
    }
    let clips = ui
        .snapshot
        .with(|s| s.as_ref().map(|s| s.clips.clone()).unwrap_or_default());
    if clips.is_empty() {
        return;
    }
    let at = clips
        .iter()
        .position(|c| c.id == ui.selected.get())
        .map(|i| i as isize)
        .unwrap_or(-1);
    let index = (at + delta).clamp(0, clips.len() as isize - 1) as usize;
    let id = clips[index].id.clone();
    ui.selected.set(id.clone());
    ui.history_list
        .scroll_to_index(index as u32, percentage(0.5), px(0.0), false);
    ui.send(Command::Select(id));
}
fn sidebar(ui: Ui) -> impl IntoElement {
    let u = ui.clone();
    let s = ui.clone();
    let d = ui.clone();
    let q = ui.clone();
    let settings = ui.clone();
    let mut nav = view()
        .p_5()
        .gap_2()
        .child(
            text("yank")
                .text_3xl()
                .font_weight(FontWeight::BOLD)
                .text_color(move || theme().primary),
        )
        .child(hint("随时找回，接着使用。"))
        .child(view().h(px(24.0)))
        .child(action("全部记录", false, move || {
            u.page.set("history".into());
            u.filter.set("all".into());
            u.send(Command::Search(u.query.get(), "all".into(), 100));
        }))
        .child(action("收藏", false, move || {
            s.page.set("history".into());
            s.filter.set("pinned".into());
            s.send(Command::Search(s.query.get(), "pinned".into(), 100));
        }))
        .child(action("设备与同步", false, move || {
            d.page.set("devices".into())
        }))
        .child(action("顺序复制", false, move || {
            q.page.set("queue".into())
        }))
        .child(view().h(px(16.0)))
        .child(hint("内容类型"));
    for (label, filter) in [
        ("文字", "text"),
        ("链接", "link"),
        ("图片", "image"),
        ("文件", "files"),
        ("代码", "code"),
        ("颜色", "color"),
    ] {
        let u = ui.clone();
        nav = nav.child(action(label, false, move || {
            u.page.set("history".into());
            u.filter.set(filter.into());
            u.send(Command::Search(u.query.get(), filter.into(), 100));
        }));
    }
    let views = ui.clone();
    let boards = ui.clone();
    let select = ui.clone();
    let boardselect = ui.clone();
    nav = nav
        .child(hint("智能列表"))
        .child(each(
            move || {
                views
                    .snapshot
                    .with(|s| s.as_ref().map(|s| s.views.clone()).unwrap_or_default())
            },
            |v: &yank_core::SavedView| v.id.clone(),
            move |item: ReadSignal<yank_core::SavedView>, _| {
                let ui = select.clone();
                action(
                    move || item.get().name,
                    false,
                    move || {
                        let filter = format!("smart:{}", item.get().id);
                        ui.filter.set(filter.clone());
                        ui.page.set("history".into());
                        ui.send(Command::Search(ui.query.get(), filter, 100));
                    },
                )
            },
        ))
        .child(hint("收藏板"))
        .child(each(
            move || {
                boards
                    .snapshot
                    .with(|s| s.as_ref().map(|s| s.boards.clone()).unwrap_or_default())
            },
            |v: &String| v.clone(),
            move |board: ReadSignal<String>, _| {
                let ui = boardselect.clone();
                action(
                    move || board.get(),
                    false,
                    move || {
                        ui.filter.set(board.get());
                        ui.page.set("history".into());
                        ui.send(Command::Search(ui.query.get(), board.get(), 100));
                    },
                )
            },
        ));
    let stats = ui.clone();
    view()
        .w(px(214.0))
        .min_h_0()
        .bg(move || theme().secondary)
        .border_r_1()
        .border_color(move || theme().border)
        .child(scroll().flex_1().min_h_0().child(nav))
        .child(
            view()
                .p_4()
                .gap_2()
                .child(hint(move || {
                    stats.snapshot.with(|s| {
                        s.as_ref()
                            .map(|s| {
                                format!("{} 条记录 · {} 条收藏", s.stats.clips, s.stats.pinned)
                            })
                            .unwrap_or_else(|| "正在打开本地资料库…".into())
                    })
                }))
                .child(action("设置与账号", false, move || {
                    settings.page.set("settings".into())
                })),
        )
}
fn mobile_nav(ui: Ui) -> impl IntoElement {
    let mut row = view().flex_row().flex_wrap().gap_2().px_3().py_2();
    for (label, page) in [
        ("记录", "history"),
        ("设备", "devices"),
        ("列表", "lists"),
        ("队列", "queue"),
        ("设置", "settings"),
    ] {
        let u = ui.clone();
        row = row.child(action(label, false, move || u.page.set(page.into())).flex_1());
    }
    row
}
fn header(ui: Ui) -> impl IntoElement {
    let capture = ui.clone();
    let prefs = ui.clone();
    let toggle_ui = ui.clone();
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .px_5()
        .py_4()
        .border_b_1()
        .border_color(move || theme().border)
        .child(
            view()
                .gap_1()
                .child(
                    text("我的剪贴板")
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(hint(move || {
                    let p = prefs.prefs.get();
                    if !is_desktop() {
                        if p.token.is_empty() {
                            "前台手动记录 · 无需账号".into()
                        } else {
                            format!("{} · 前台手动记录", p.username)
                        }
                    } else if p.token.is_empty() {
                        "本地优先 · 无需账号".into()
                    } else {
                        format!(
                            "{} · {}",
                            p.username,
                            if p.upload {
                                "服务器存储已开启"
                            } else {
                                "不上传存储"
                            }
                        )
                    }
                })),
        )
        .child(action("记录剪贴板", true, move || {
            capture_now(capture.clone())
        }))
        .child(when(is_desktop, move || {
            let label = ui.clone();
            let toggle_ui = toggle_ui.clone();
            action(
                move || {
                    if label.prefs.get().capture {
                        "暂停记录"
                    } else {
                        "继续记录"
                    }
                },
                false,
                move || {
                    let mut p = toggle_ui.prefs.get();
                    p.capture = !p.capture;
                    toggle_ui.save(p);
                },
            )
        }))
}
fn capture_now(ui: Ui) {
    let source = if is_desktop() {
        CaptureSource::System
    } else {
        CaptureSource::Foreground
    };
    spawn(async move {
        let value = clipboard_string().await;
        let files = if is_desktop() {
            clipboard_files().await
        } else {
            Vec::new()
        };
        if !files.is_empty() {
            ui.send(Command::Capture(
                Clip::from_formats("", &files.join("\n"), vec![ClipFormat::file_list(&files)]),
                source,
            ));
        } else if !value.is_empty() {
            ui.send(Command::Capture(Clip::text("", &value), source));
        } else {
            ui.notice.set(
                if is_desktop() {
                    "剪贴板中没有可读取的文字或文件；桌面图片会自动记录。"
                } else {
                    "剪贴板中没有可读取的文字。移动端仅支持前台手动记录文字。"
                }
                .into(),
            );
        }
    });
}
fn history(ui: Ui, desktop: Memo<bool>) -> impl IntoElement {
    if is_desktop() {
        let search_focused = signal(false);
        ui.search_field
            .on_focus_in(move || search_focused.set(true));
        ui.search_field
            .on_focus_out(move || search_focused.set(false));
        let state = ui.clone();
        effect(move || {
            // Only the search input opts into navigation while editing. The
            // inline details editor keeps its arrow and number shortcuts.
            let when = if search_focused.get() {
                nsh_core::keymap::When::Always
            } else {
                nsh_core::keymap::When::NotEditing
            };
            for (key, delta) in [("arrowdown", 1), ("arrowup", -1)] {
                let state = state.clone();
                on_key_with(
                    key,
                    KeyOptions {
                        when,
                        repeat: true,
                        ..Default::default()
                    },
                    move |_| navigate(&state, delta),
                );
            }
            for (index, digit) in ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"]
                .iter()
                .enumerate()
            {
                let state = state.clone();
                on_key_with(
                    &format!("mod+{digit}"),
                    KeyOptions {
                        when,
                        ..Default::default()
                    },
                    move |_| desktop::paste_selected(state.clone(), Some(index)),
                );
            }
        });
        let list_focused = signal(false);
        ui.history_list.on_focus_in(move || list_focused.set(true));
        ui.history_list
            .on_focus_out(move || list_focused.set(false));
        let state = ui.clone();
        effect(move || {
            if list_focused.get() {
                let state = state.clone();
                on_key_with(
                    "enter",
                    KeyOptions {
                        when: nsh_core::keymap::When::NotEditing,
                        ..Default::default()
                    },
                    move |_| desktop::paste_selected(state.clone(), None),
                );
            }
        });
    }
    let query = ui.clone();
    let change = ui.clone();
    let rows = ui.clone();
    let keys = ui.clone();
    let row = ui.clone();
    let info = ui.clone();
    let more = ui.clone();
    let detail = ui.clone();
    view()
        .flex_1()
        .min_h_0()
        .p_4()
        .gap_3()
        .child(
            field(
                "搜索文字、标题或收藏板",
                move || query.query.get(),
                move |value| {
                    change.query.set(value.into());
                    change.limit.set(100);
                    change.selected.set(String::new());
                    change.send(Command::Search(value.into(), change.filter.get(), 100));
                },
            )
            .element_ref(&ui.search_field)
            .auto_focus(is_desktop())
            .on_submit({
                let ui = ui.clone();
                move |_| desktop::paste_selected(ui.clone(), None)
            }),
        )
        .child(action("保存筛选为智能列表", false, {
            let ui = ui.clone();
            move || ui.page.set("smart".into())
        }))
        .child(hint(move || {
            info.snapshot.with(|s| {
                s.as_ref()
                    .map(|s| {
                        if is_desktop() {
                            format!(
                                "{} 条结果 · ↑ ↓ 选择 · Enter 粘贴 · {}+1…0 快贴前十条 · Esc 收起",
                                s.clips.len(),
                                if apple_keys() { "⌘" } else { "Ctrl" }
                            )
                        } else {
                            format!("{} 条结果", s.clips.len())
                        }
                    })
                    .unwrap_or_else(|| "正在读取本地历史…".into())
            })
        }))
        .child(
            view()
                .flex_row()
                .flex_1()
                .min_h_0()
                .gap_4()
                .child(
                    view()
                        .flex_1()
                        .min_w_0()
                        .child(
                            virtual_list(
                                move || {
                                    rows.snapshot
                                        .with(|s| s.as_ref().map(|s| s.clips.len()).unwrap_or(0))
                                },
                                move |at| clip_row(row.clone(), at),
                            )
                            .keys(move || {
                                keys.snapshot.with(|s| {
                                    s.as_ref()
                                        .map(|s| {
                                            s.clips.iter().map(|c| c.id.clone()).collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default()
                                })
                            })
                            .element_ref(&ui.history_list)
                            .estimated_item_size(px(92.0))
                            .flex_1()
                            .empty(
                                surface()
                                    .child(
                                        text({
                                            let ui = ui.clone();
                                            move || {
                                                if ui.query.get().is_empty()
                                                    && ui.filter.get() == "all"
                                                {
                                                    "复制过的内容，会留在这里"
                                                } else {
                                                    "没有匹配的记录"
                                                }
                                            }
                                        })
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD),
                                    )
                                    .child(hint({
                                        let ui = ui.clone();
                                        move || {
                                            if ui.query.get().is_empty() && ui.filter.get() == "all"
                                            {
                                                "先复制一段文字，或点“记录剪贴板”。不登录也能使用。"
                                            } else {
                                                "试试其他关键词，或切回全部记录。"
                                            }
                                        }
                                    })),
                            ),
                        )
                        .child(
                            action("加载更多记录", false, move || {
                                more.limit.update(|l| *l += 100);
                                more.send(Command::Search(
                                    more.query.get(),
                                    more.filter.get(),
                                    more.limit.get(),
                                ));
                            })
                            .disabled({
                                let u = ui.clone();
                                move || !u.snapshot.with(|s| s.as_ref().is_some_and(|s| s.has_more))
                            }),
                        ),
                )
                .child(when(
                    move || desktop.get(),
                    move || view().w(px(320.0)).child(details(detail.clone())),
                )),
        )
}
fn clip_row(ui: Ui, at: Position) -> impl IntoElement {
    let current = computed({
        let u = ui.clone();
        move || {
            u.snapshot
                .with(|s| s.as_ref().and_then(|s| s.clips.get(at.index()).cloned()))
        }
    });
    let choose = ui.clone();
    let selected = ui.clone();
    let focused = ui.clone();
    let row_ref = element_ref();
    row_ref.on_focus_in(move || {
        if let Some(clip) = current.get()
            && focused.selected.get() != clip.id
        {
            focused.pending_float.set(None);
            focused.selected.set(clip.id.clone());
            focused.send(Command::Select(clip.id));
        }
    });
    nsh_base::button()
        .element_ref(&row_ref)
        .on_press(move || {
            if let Some(clip) = current.get() {
                choose.choose(clip);
            }
        })
        .p_4()
        .gap_2()
        .border_b_1()
        .border_color(move || theme().border)
        .min_h(px(88.0))
        .items_start()
        .bg(move || {
            if current
                .get()
                .is_some_and(|c| c.id == selected.selected.get())
            {
                theme().accent
            } else {
                theme().card
            }
        })
        .hover(|s| s.bg(move || theme().muted))
        .active(|s| s.opacity(0.7))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .accessibility_label(move || current.get().map(|c| SharedString::from(c.description)))
        .child(
            view()
                .flex_row()
                .gap_2()
                .items_center()
                .child(when(
                    move || is_desktop() && at.index() < 10,
                    move || {
                        text(format!("{}", (at.index() + 1) % 10))
                            .text_xs()
                            .text_color(move || theme().muted_foreground)
                    },
                ))
                .child(
                    text(move || {
                        current
                            .get()
                            .map(|c| {
                                format!(
                                    "{}{}",
                                    if c.pinned { "已收藏 · " } else { "" },
                                    c.description
                                )
                            })
                            .unwrap_or_default()
                    })
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
                ),
        )
        .child(
            text(move || {
                current
                    .get()
                    .map(|c| {
                        if c.text.is_empty() {
                            format!("{} · {} 个格式", c.kind, c.formats.len())
                        } else {
                            c.text.chars().take(110).collect()
                        }
                    })
                    .unwrap_or_default()
            })
            .text_xs()
            .text_color(move || theme().muted_foreground),
        )
}
fn details(ui: Ui) -> impl IntoElement {
    let get = ui.clone();
    let text = ui.clone();
    let draft = ui.clone();
    let edit = ui.clone();
    let collection = ui.clone();
    let set_collection = ui.clone();
    let collection_ready = ui.clone();
    let copy = ui.clone();
    let pin = ui.clone();
    let queue = ui.clone();
    let save = ui.clone();
    let delete = ui.clone();
    let ditto = ui.clone();
    let float = ui.clone();
    let preview = ui.clone();
    let readonly = ui.clone();
    let image_ui = ui.clone();
    scroll().flex_1().child(
        surface()
            .child(title("内容详情"))
            .child(hint(move || {
                get.selected()
                    .map(|c| format!("{} · {} 个格式", c.kind, c.formats.len()))
                    .unwrap_or_else(|| "选择一条记录，查看完整内容。".into())
            }))
            .child(
                field(
                    "编辑文字",
                    move || draft.draft().map(|d| d.text).unwrap_or_default(),
                    move |s| edit.edit_draft(|d| d.text = s.into()),
                )
                .multiline(true)
                .min_h(px(170.0)).readonly(move||readonly.selected().is_none_or(|c| !text_editable(&c))),
            )
            .child(field(
                "收藏板名称",
                move || collection.draft().map(|d| d.collection).unwrap_or_default(),
                move |s| set_collection.edit_draft(|d| d.collection = s.into()),
            ).readonly(move || collection_ready.selected().is_none()))
            .child(hint({let ui=ui.clone();move || if ui.selected().is_some_and(|c| !text_editable(&c)) { "含二进制或文件格式：仅可修改收藏板，复制使用完整已保存格式。" } else { "编辑文字后保存将以纯文本存储。" }}))
            .child(hint(move || {
                text.selected()
                    .map(|c| format!("记录 ID：{}", c.id))
                    .unwrap_or_default()
            }))
            .child(when(move||image_ui.selected().is_some_and(|c|c.kind=="image"),{let ui=ui.clone();move||{let ui=ui.clone();image(move||{use base64::Engine;ui.selected().and_then(|c|c.formats.into_iter().find(|f|f.mime.as_deref()==Some("image/png"))).map(|f|format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(f.data))).unwrap_or_default()}).h(px(180.0))}}))
            .child(action("Markdown / 代码预览",false,move||preview.page.set("preview".into())))
            .child(action("悬浮为置顶卡片",false,move||{if let Some(c)=float.selected(){desktop::floating(float.clone(),c);}}))
            .child(hint({ let ui = ui.clone(); move || if ui.dirty() { "未保存 · 切换记录或返回后草稿仍保留" } else { "已保存" } }))
            .child(when(is_desktop, { let ui = ui.clone(); move || {
                action("粘贴到原应用", true, { let ui = ui.clone(); move || {
                    if let Some(clip) = ui.selected() { desktop::paste_record(ui.clone(), clip.id); }
                } })
                    .disabled({ let ui = ui.clone(); move || ui.selected().is_none_or(|clip| clip.deleted) })
            }}))
            .child(action("复制已保存内容", false, move || copy.copy()))
            .child(action("复制草稿文字", false, { let ui = ui.clone(); move || {
                if let Some(mut clip) = ui.selected().filter(text_editable)
                    && let Some(draft) = ui.draft() {
                    clip.text = draft.text;
                    clip.formats = vec![ClipFormat::text(&clip.text)];
                    ui.copy_clip(clip);
                    ui.notice.set("已复制草稿文字；尚未保存到记录".into());
                }
            }}).disabled({ let ui = ui.clone(); move || !ui.selected().is_some_and(|c| text_editable(&c)) }))
            .child(action("收藏 / 取消收藏", false, move || {
                if let Some(c) = pin.selected() {
                    pin.send(Command::Pin(c.id));
                }
            }))
            .child(action("加入顺序复制", false, move || {
                if let Some(c) = queue.selected() {
                    queue.queue.update(|q| {if q.len()<1000 && !q.iter().any(|item|item.id==c.id){q.push(c);}});
                    queue.notice.set("已加入顺序复制".into());
                }
            }))
            .child(action("保存编辑", false, move || {
                if let Some(c) = save.selected() && let Some(draft) = save.draft() {
                    let text = if text_editable(&c) { draft.text } else { c.text.clone() };
                    save.send(Command::Edit(c.id, text, draft.collection));
                }
            }))
            .child(action("发送到远端 Ditto", false, move || {
                if let Some(clip) = ditto.selected() {
                    ditto.send(Command::SendDitto(clip));
                }
            }).disabled({let ui = ui.clone(); move || ui.pending()}))
            .child(form_feedback(ui.clone()))
            .child(action("删除记录", false, move || {
                if let Some(c) = delete.selected() {
                    let u=delete.clone();let answer=alert_dialog().title("删除这条记录？").description("删除状态会同步到已配对设备；启用服务器存储时也会同步到服务器。请确认后删除。").cancel("cancel","取消").destructive("delete","删除记录").show();spawn(async move{if answer.await.is_ok_and(|a|a.action.as_deref()==Some("delete")){u.send(Command::Delete(c.id.clone()));u.selected.set(String::new());u.drafts.update(|drafts| { drafts.remove(&c.id); });}});
                }
            })),
    )
}
fn queue(ui: Ui) -> impl IntoElement {
    let count = ui.clone();
    let next = ui.clone();
    let clear = ui.clone();
    let list = ui.clone();
    let collecting = ui.clone();
    let set_collecting = ui.clone();
    scroll().flex_1().p_5().child(
        view()
            .gap_4()
            .child(title("顺序复制"))
            .child(toggle("自动收集本机剪贴板", move || collecting.collecting.get(), move |enabled| {
                set_collecting.worker.shared.collecting.store(enabled, Ordering::Relaxed);
                set_collecting.collecting.set(enabled);
            }))
            .child(hint("开启后会收集当前剪贴板及后续本机捕获，最多 1000 条；同步历史不会加入。关闭后保留队列。安卓仅收集前台手动记录。"))
            .child(hint(
                "先把记录加入队列，再逐条复制。桌面可用全局快捷键把下一条粘贴到当前应用。",
            ))
            .child(hint(move || {
                format!("队列中还有 {} 条", count.queue.with(Vec::len))
            }))
            .child(action("复制下一条", true, move || {
                let clip = next.queue.with(|q| q.first().cloned());
                if clip.is_some() {
                    next.queue.update(|q| {
                        q.remove(0);
                    });
                }
                if let Some(clip) = clip {
                    next.copy_clip(clip);
                }
            }))
            .child(action("清空队列", false, move || {
                clear.queue.set(Vec::new())
            }))
            .child(each(
                move || list.queue.get(),
                |c: &Clip| c.id.clone(),
                move |c, _| surface().child(text(move || c.get().description).text_sm()),
            )),
    )
}
fn devices(ui: Ui) -> impl IntoElement {
    let identity = ui.clone();
    let code = ui.clone();
    let value = ui.clone();
    let pair = ui.clone();
    let found = ui.clone();
    let trusted = ui.clone();
    let conflict = ui.clone();
    let instances = ui.clone();
    scroll().flex_1().p_5().child(
        view()
            .gap_5()
            .child(title("设备与同步"))
            .child(
                surface()
                    .child(hint("本机设备码"))
                    .child(
                        text(move || {
                            identity.snapshot.with(|snapshot| {
                                snapshot
                                    .as_ref()
                                    .and_then(|snapshot| snapshot.identity.as_ref())
                                    .map(|peer| peer.code.clone())
                                    .unwrap_or_else(|| "正在生成…".into())
                            })
                        })
                        .text_2xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().primary),
                    )
                    .child(hint(
                        "设备互通与账号归属相互独立。配对前需要对方确认，可以选择不关联账号。",
                    )),
            )
            .child(
                surface()
                    .child(title("使用设备码配对"))
                    .child(field(
                        "输入对方的设备码",
                        move || code.code.get(),
                        move |s| value.code.set(s.to_uppercase()),
                    ))
                    .child(action("请求配对", true, move || {
                        let u = pair.clone();
                        let mut dialog = alert_dialog()
                            .title("使用设备码配对")
                            .description("可以仅连接两台设备。账号关联需要单独同意。")
                            .cancel("cancel", "取消")
                            .action("pair", "仅设备互通");
                        if !u.prefs.get().token.is_empty() {
                            dialog = dialog.action("account", "邀请关联当前账号");
                        }
                        let answer = dialog.show();
                        spawn(async move {
                            let choice = answer.await.ok().and_then(|a| a.action);
                            if matches!(choice.as_deref(), Some("pair" | "account")) {
                                u.send(Command::PairCode(
                                    u.code.get(),
                                    choice.as_deref() == Some("account"),
                                    0,
                                ));
                            }
                        });
                    })),
            )
            .child(title("局域网 Yank 服务"))
            .child(each(
                move || {
                    let _ = instances.snapshot.get();
                    instances
                        .worker
                        .shared
                        .servers
                        .lock()
                        .map(|s| s.values().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                },
                |s: &crate::network::ServerInstance| s.url.clone(),
                {
                    let ui = ui.clone();
                    move |server: ReadSignal<crate::network::ServerInstance>, _| {
                        let ui = ui.clone();
                        surface()
                            .child(
                                text(move || {
                                    format!("{} · {}", server.get().name, server.get().url)
                                })
                                .text_sm(),
                            )
                            .child(action("选择是否关联账号", false, move || {
                                let selected = server.get();
                                let ui = ui.clone();
                                let answer = alert_dialog()
                                    .title("发现 Yank 服务")
                                    .description(format!(
                                        "{}\n可以登录关联账号，也可以只使用发现与中继。",
                                        selected.url
                                    ))
                                    .cancel("cancel", "暂不使用")
                                    .action("relay", "仅使用发现与中继")
                                    .action("login", "登录并关联账号")
                                    .show();
                                spawn(async move {
                                    let choice = answer.await.ok().and_then(|a| a.action);
                                    if choice.as_deref() == Some("relay") {
                                        let mut p = ui.prefs.get();
                                        p.detach_account();
                                        p.server_url = selected.url.clone();
                                        ui.save(p);
                                        ui.server.set(selected.url);
                                    } else if choice.as_deref() == Some("login") {
                                        ui.server.set(selected.url);
                                        ui.page.set("settings".into());
                                    }
                                });
                            }))
                    }
                },
            ))
            .child(title("发现的设备"))
            .child(each(
                move || {
                    let _ = found.snapshot.get();
                    found
                        .worker
                        .shared
                        .discovered
                        .lock()
                        .map(|d| d.values().cloned().collect::<Vec<_>>())
                        .unwrap_or_default()
                },
                |d: &crate::network::Discovered| d.peer.id.clone(),
                {
                    let ui = ui.clone();
                    move |peer: ReadSignal<crate::network::Discovered>, _| {
                        let p = peer;
                        let u = ui.clone();
                        surface()
                            .child(
                                text(move || format!("{} · {}", p.get().peer.name, p.get().via))
                                    .text_base(),
                            )
                            .child(hint(move || p.get().peer.code))
                            .child(action("配对并选择关联方式", false, move || {
                                confirm_pair(u.clone(), p.get().peer)
                            }))
                    }
                },
            ))
            .child(title("已绑定设备"))
            .child(each(
                move || {
                    trusted
                        .snapshot
                        .with(|s| s.as_ref().map(|s| s.peers.clone()).unwrap_or_default())
                },
                |p: &yank_core::Peer| p.id.clone(),
                {
                    let u = ui.clone();
                    move |peer: ReadSignal<yank_core::Peer>, _| {
                        let u = u.clone();
                        surface()
                            .child(text(move || peer.get().name).text_sm())
                            .child(action("解除绑定", false, move || {
                                u.send(Command::Unpair(peer.get().id))
                            }))
                    }
                },
            ))
            .child(title("并发修改的保留版本"))
            .child(hint(
                "遇到同时编辑时会保留另一份内容。恢复会生成新版本，并同步到已连接设备。",
            ))
            .child(each(
                move || {
                    conflict
                        .snapshot
                        .with(|s| s.as_ref().map(|s| s.conflicts.clone()).unwrap_or_default())
                },
                |p: &(String, Clip)| p.0.clone(),
                {
                    let u = ui.clone();
                    move |record: ReadSignal<(String, Clip)>, _| {
                        let u = u.clone();
                        surface()
                            .child(text(move || record.get().1.description).text_sm())
                            .child(action("恢复此版本", false, move || {
                                u.send(Command::Resolve(record.get().0))
                            }))
                    }
                },
            )),
    )
}
fn confirm_pair(ui: Ui, peer: yank_core::Peer) {
    let mut dialog = alert_dialog()
        .title(format!("连接 {}？", peer.name))
        .description(format!(
            "请核对设备码 {}。对方确认后可交换剪贴板数据；可以选择不关联账号。",
            peer.code
        ))
        .cancel("cancel", "取消")
        .action("pair", "仅设备互通");
    if !ui.prefs.get().token.is_empty() {
        dialog = dialog.action("account", "邀请关联当前账号");
    }
    let answer = dialog.show();
    spawn(async move {
        let choice = answer.await.ok().and_then(|a| a.action);
        if matches!(choice.as_deref(), Some("pair" | "account")) {
            let account = if choice.as_deref() == Some("account") {
                let p = ui.prefs.get();
                Some(Association {
                    server_url: p.server_url,
                    username: p.username,
                })
            } else {
                None
            };
            match ui.worker.shared.queue_pair(peer, account, None) {
                Ok(()) => ui.notice.set("配对请求已发送，等待对方确认。".into()),
                Err(error) => ui.notice.set(format!("无法发送配对请求：{error}")),
            }
        }
    });
}
fn settings(ui: Ui) -> impl IntoElement {
    let server = ui.clone();
    let set_server = ui.clone();
    let username = ui.clone();
    let set_username = ui.clone();
    let password = ui.clone();
    let set_password = ui.clone();
    let login = ui.clone();
    let logout = ui.clone();
    let guest = ui.clone();
    let upload = ui.clone();
    let set_upload = ui.clone();
    let mut choices = nsh_base::radio_group().gap_2();
    for (label, mode) in [
        ("默认中继", "relay"),
        ("仅直连", "direct"),
        ("自动 NAT / 中继保底", "auto"),
    ] {
        let u = ui.clone();
        let checked = ui.clone();
        choices = choices.child(
            nsh_base::radio()
                .checked(move || checked.prefs.get().mode == mode)
                .on_change(move |_: &bool| {
                    let mut p = u.prefs.get();
                    p.mode = mode.into();
                    u.save(p);
                })
                .min_h(px(48.0))
                .p_3()
                .border_1()
                .rounded(move || theme().radius)
                .border_color(move || theme().border)
                .bg(move || theme().card)
                .checked_style(|style| style.bg(theme().accent).border_color(theme().primary))
                .focus(|style| style.outline(px(2.0)).outline_color(theme().ring))
                .child(text(label).text_sm().text_color(move || theme().foreground)),
        );
    }
    let ditto = ui.clone();
    let set_ditto = ui.clone();
    let theme_ui = ui.clone();
    let theme_label = ui.clone();
    let export = ui.clone();
    let import = ui.clone();
    scroll().flex_1().p_5().child(view().gap_5().max_w(px(760.0))
      .child(title("设置与账号"))
      .child(surface().child(title("这台设备"))
        .child(pref_text(ui.clone(), "设备名称", "保存设备名称", |p|p.device_name.clone(), |p,v|p.device_name=v.into()))
        .child(action(move||format!("外观：{}（点击切换）",match theme_label.prefs.get().theme.as_str(){"dark"=>"深色","light"=>"浅色",_=>"跟随系统"}),false,move||{let mut p=theme_ui.prefs.get();p.theme=match p.theme.as_str(){"system"=>"light","light"=>"dark",_=>"system"}.into();theme_ui.save(p);})))
      .child(surface().child(title("账号登录（可选）"))
        .child(hint("离线记录不需要账号。登录用于发现同账号设备；服务器存储仍需单独启用。"))
        .child(field("服务器地址，例如 https://yank.example.com",move||server.server.get(),move|s|set_server.server.set(s.into())))
        .child(field("用户名",move||username.username.get(),move|s|set_username.username.set(s.into())))
        .child(field("密码",move||password.password.get(),move|s|set_password.password.set(s.into())).masked(true))
        .child(action("登录并关联此设备",true,move||{login.send(Command::Login(login.server.get(),login.username.get(),login.password.get(),0));}).disabled({let ui=ui.clone();move||ui.pending()}))
        .child(form_feedback(ui.clone()))
        .child(action("仅使用发现与中继，不关联账号",false,move||{let mut p=guest.prefs.get();p.detach_account();p.server_url=guest.server.get();guest.save(p);}))
        .child(action("退出账号，继续离线使用",false,move||logout.send(Command::Logout))))
      .child(surface().child(title("数据存储"))
        .child(toggle("上传并保存在服务端",move||upload.prefs.get().upload,move|enabled|{
            if enabled {if set_upload.prefs.get().token.is_empty(){set_upload.notice.set("请先登录账号；本地历史仍可正常使用。".into());return;}
                let u=set_upload.clone();let answer=alert_dialog().title("启用服务器存储？").description("本机历史及后续修改将上传到当前账号的服务器存储。设备中继不需要此选项。关闭后，已上传内容仍保留在服务器。").cancel("cancel","取消").action("enable","启用存储").show();
                spawn(async move{if answer.await.is_ok_and(|a|a.action.as_deref()==Some("enable")){let mut p=u.prefs.get();p.upload=true;u.save(p);}});
            }else{let mut p=set_upload.prefs.get();p.upload=false;set_upload.save(p);}
        }))
        .child(hint("默认关闭。关闭后不再上传新数据，已发出的请求可能完成；已有远端数据请在管理页删除。")))
      .child(surface().child(title("设备传输方式")).child(choices)
        .child(hint("仅直连不经过中继；自动模式尝试 NAT 穿透并保留中继回退。实际链路状态显示在窗口底部。"))
        .child(pref_text(ui.clone(), "中继地址（留空使用服务器提供的地址）", "保存中继地址", |p|p.relay_url.clone(), |p,v|p.relay_url=v.into())))
      .child(surface().child(title("Ditto 插件（rutis）"))
        .child(toggle("接收 Ditto 推送",move||ditto.prefs.get().ditto_enabled,move|enabled|{let mut p=set_ditto.prefs.get();p.ditto_enabled=enabled;set_ditto.save(p);}))
        .child(pref_text(ui.clone(), "Ditto 监听地址", "保存 Ditto 监听地址", |p|p.ditto_listen.clone(), |p,v|p.ditto_listen=v.into()))
        .child(pref_text(ui.clone(), "Ditto 共享密码", "保存 Ditto 共享密码", |p|p.ditto_password.clone(), |p,v|p.ditto_password=v.into()).masked(true))
        .child(pref_text(ui.clone(), "远端 Ditto 地址（主机:端口）", "保存远端 Ditto 地址", |p|p.ditto_remote.clone(), |p,v|p.ditto_remote=v.into()))
        .child(hint("默认关闭，监听本机 23443。接收远端推送时，请在 Ditto 中填写本机可达地址和相同共享密码。")))
      .child(extra_preferences(ui.clone()))
      .child(surface().child(title("隐私与备份"))
        .child(hint("安卓采用前台手动记录，遵循系统剪贴板访问限制。排除表达式匹配的文字不会进入历史。"))
        .child(pref_text(ui.clone(), "排除内容的正则表达式（可留空）", "保存排除正则", |p|p.exclude_pattern.clone(), |p,v|p.exclude_pattern=v.into()))
        .child(action("导出全部本地记录",false,move||{let u=export.clone(); if u.pending() {return;} u.picking.set(true); spawn(async move{let result=pick_save_path(nsh_core::file_dialog::SaveDialog::new().file_name("yank-backup.json")).await;u.picking.set(false);match result {Ok(Some(path))=>u.send(Command::Export(path)),Err(error)=>u.notice.set(format!("无法选择导出文件：{error}")),_=>{}}});}))
        .child(action("导入备份",false,move||{let u=import.clone(); if u.pending() {return;} u.picking.set(true); spawn(async move{let result=pick_paths(nsh_core::file_dialog::OpenDialog::files()).await;u.picking.set(false);match result {Ok(Some(paths))=>{if let Some(path)=paths.into_iter().next(){u.send(Command::Import(path));}},Err(error)=>u.notice.set(format!("无法选择导入文件：{error}")),_=>{}}});}).disabled({let ui=ui.clone();move||ui.pending()}))
        .child(recovery_controls(ui.clone()))
        .child(form_feedback(ui.clone()))))
}

fn preview(ui: Ui) -> impl IntoElement {
    let text = ui.clone();
    let back = ui.clone();
    scroll().flex_1().p_5().child(
        surface()
            .child(title("内容预览"))
            .child(
                safe_text_view()
                    .markdown(move || text.selected().map(|c| SharedString::from(c.text)))
                    .selectable(true)
                    .text_color(move || theme().foreground),
            )
            .child(action("返回历史", false, move || {
                back.page.set("history".into())
            })),
    )
}

fn smart_editor(ui: Ui) -> impl IntoElement {
    let name = ui.clone();
    let set = ui.clone();
    let save = ui.clone();
    let days = ui.clone();
    let back = ui.clone();
    let remove = ui.clone();
    scroll().flex_1().p_5().child(
        surface()
            .child(title("智能列表"))
            .child(hint("保存当前关键词和内容类型，之后一键打开。"))
            .child(field(
                "列表名称",
                move || name.smart_name.get(),
                move |s| set.smart_name.set(s.into()),
            ))
            .child(action(
                move || {
                    format!(
                        "时间范围：{}",
                        match days.smart_days.get() {
                            0 => "不限",
                            7 => "最近一周",
                            30 => "最近一月",
                            _ => "最近一年",
                        }
                    )
                },
                false,
                {
                    let ui = ui.clone();
                    move || {
                        ui.smart_days.update(|days| {
                            *days = match *days {
                                0 => 7,
                                7 => 30,
                                30 => 365,
                                _ => 0,
                            }
                        })
                    }
                },
            ))
            .child(action("保存智能列表", true, move || {
                let title = save.smart_name.get();
                if title.trim().is_empty() {
                    save.notice.set("请输入列表名称".into());
                    return;
                }
                let filter = save.filter.get();
                let previous = save.snapshot.with(|s| {
                    s.as_ref().and_then(|s| {
                        s.views
                            .iter()
                            .find(|v| format!("smart:{}", v.id) == filter)
                            .cloned()
                    })
                });
                let (query, filter) = previous
                    .map(|v| (v.query, v.filter))
                    .unwrap_or_else(|| (save.query.get(), filter));
                save.send(Command::SaveView(yank_core::SavedView {
                    id: yank_core::new_id(),
                    name: title,
                    query,
                    filter,
                    days: save.smart_days.get(),
                    icon: "list".into(),
                }));
                save.smart_name.set(String::new());
                save.page.set("history".into());
            }))
            .child(action("删除当前智能列表", false, move || {
                if let Some(id) = remove.filter.get().strip_prefix("smart:") {
                    remove.send(Command::RemoveView(id.into()));
                    remove.filter.set("all".into());
                    remove.send(Command::Search(String::new(), "all".into(), 100));
                    remove.page.set("history".into());
                }
            }))
            .child(action("返回历史", false, move || {
                back.page.set("history".into())
            })),
    )
}

fn form_feedback(ui: Ui) -> impl IntoElement {
    let message = ui.clone();
    text(move || message.feedback())
        .text_sm()
        .text_color(move || {
            if ui.snapshot.with(|snapshot| {
                snapshot
                    .as_ref()
                    .is_some_and(|snapshot| !snapshot.error.is_empty())
            }) {
                theme().destructive
            } else {
                theme().muted_foreground
            }
        })
}

fn safe_text_view() -> nsh_base::TextViewElement {
    nsh_base::text_view()
        .options(nsh_base::text_view::MarkdownOptions {
            html: false,
            ..Default::default()
        })
        .render_image(|_: &nsh_base::text_view::Image| {
            hint("图片已阻止加载 · 预览不会请求远端图片")
        })
}

fn lists(ui: Ui) -> impl IntoElement {
    let views = ui.clone();
    let boards = ui.clone();
    let select = ui.clone();
    let boardselect = ui.clone();
    scroll().flex_1().p_4().child(
        surface()
            .child(title("列表与收藏板"))
            .child(hint("智能列表"))
            .child(each(
                move || {
                    views
                        .snapshot
                        .with(|s| s.as_ref().map(|s| s.views.clone()).unwrap_or_default())
                },
                |v: &yank_core::SavedView| v.id.clone(),
                move |item, _| {
                    let ui = select.clone();
                    action(
                        move || item.get().name,
                        false,
                        move || {
                            let filter = format!("smart:{}", item.get().id);
                            ui.filter.set(filter.clone());
                            ui.page.set("history".into());
                            ui.send(Command::Search(ui.query.get(), filter, 100));
                        },
                    )
                },
            ))
            .child(hint("收藏板"))
            .child(each(
                move || {
                    boards
                        .snapshot
                        .with(|s| s.as_ref().map(|s| s.boards.clone()).unwrap_or_default())
                },
                |v: &String| v.clone(),
                move |item, _| {
                    let ui = boardselect.clone();
                    action(
                        move || item.get(),
                        false,
                        move || {
                            let filter = item.get();
                            ui.filter.set(filter.clone());
                            ui.page.set("history".into());
                            ui.send(Command::Search(ui.query.get(), filter, 100));
                        },
                    )
                },
            )),
    )
}

fn pref_text(
    ui: Ui,
    label: &'static str,
    save_label: &'static str,
    get: fn(&Preferences) -> String,
    set: fn(&mut Preferences, &str),
) -> controls::Field {
    let read = ui.clone();
    let edit = ui.clone();
    let enabled = ui.clone();
    field(
        label,
        move || {
            read.preference_drafts
                .with(|drafts| drafts.get(label).cloned())
                .unwrap_or_else(|| get(&read.prefs.get()))
        },
        move |value| {
            edit.preference_drafts.update(|drafts| {
                drafts.insert(label, value.into());
            });
        },
    )
    .action(
        action(save_label, false, move || {
            if let Some(value) = ui
                .preference_drafts
                .with(|drafts| drafts.get(label).cloned())
            {
                let mut prefs = ui.prefs.get();
                set(&mut prefs, &value);
                ui.save(prefs);
            }
        })
        .disabled(move || {
            let saved = get(&enabled.prefs.get());
            enabled.pending()
                || enabled
                    .preference_drafts
                    .with(|drafts| drafts.get(label).is_none_or(|draft| draft == &saved))
        }),
    )
}
fn pref_toggle(
    ui: Ui,
    label: &'static str,
    get: fn(&Preferences) -> bool,
    set: fn(&mut Preferences, bool),
) -> impl IntoElement {
    let read = ui.clone();
    toggle(
        label,
        move || get(&read.prefs.get()),
        move |value| {
            let mut p = ui.prefs.get();
            set(&mut p, value);
            ui.save(p);
        },
    )
}
fn extra_preferences(ui: Ui) -> impl IntoElement {
    let mut panel = surface().child(title("剪贴板与备份设置"))
        .child(pref_toggle(ui.clone(), "后续远端变更写入系统剪贴板", |p|p.apply_remote, |p,v|p.apply_remote=v))
        .child(hint("首次历史同步不写入系统剪贴板。此开关控制后续远端内容变更；安卓受系统限制，仅支持前台手动复制。"))
        .child(pref_text(ui.clone(), "排除的应用（每行一个）", "保存排除的应用", |p|p.excluded_apps.clone(), |p,v|p.excluded_apps=v.into()).multiline(true))
        .child(pref_toggle(ui.clone(), "每日自动备份", |p|p.backup_daily, |p,v|p.backup_daily=v));
    let read = ui.clone();
    let write = ui.clone();
    panel = panel.child(field(
        "保留天数（0 为永久）",
        move || read.prefs.get().retention_days.to_string(),
        move |value| {
            if let Ok(days) = value.parse::<u32>() {
                let mut p = write.prefs.get();
                p.retention_days = days;
                write.save(p);
            } else {
                write.notice.set("保留天数请输入非负整数".into());
            }
        },
    ));
    if is_desktop() {
        panel = panel
            .child(title("桌面快捷键与面板"))
            .child(shortcut_preferences(ui.clone()))
            .child(pref_toggle(
                ui.clone(),
                "静默启动",
                |p| p.silent_start,
                |p, v| p.silent_start = v,
            ));
        let size = ui.clone();
        let set_size = ui.clone();
        panel = panel.child(action(
            move || format!("面板大小：{} / 5（点击调整）", size.prefs.get().panel_size),
            false,
            move || {
                let mut p = set_size.prefs.get();
                p.panel_size = (p.panel_size + 1) % 6;
                set_size.save(p);
            },
        ));
        let mut positions = nsh_base::radio_group().gap_2();
        for (label, position) in [
            ("居中", "center"),
            ("顶部", "top"),
            ("底部", "bottom"),
            ("左侧", "left"),
            ("右侧", "right"),
            ("托盘附近", "tray"),
        ] {
            let read = ui.clone();
            let write = ui.clone();
            positions = positions.child(
                nsh_base::radio()
                    .checked(move || read.prefs.get().panel_position == position)
                    .on_change(move |_: &bool| {
                        let mut p = write.prefs.get();
                        p.panel_position = position.into();
                        write.save(p);
                    })
                    .min_h(px(48.0))
                    .p_3()
                    .border_1()
                    .border_color(move || theme().border)
                    .checked_style(|s| s.bg(theme().accent).border_color(theme().primary))
                    .child(text(label).text_color(move || theme().foreground)),
            );
        }
        let check = ui.clone();
        panel = panel.child(hint("面板位置")).child(positions)
            .child(hint("托盘附近：使用最近一次托盘点击提供的位置。系统未提供坐标时居中显示；Linux 不提供托盘坐标。"))
            .child(pref_toggle(ui.clone(), "登录系统时启动", |p|p.launch_at_login, |p,v|p.launch_at_login=v))
            .child(pref_toggle(ui.clone(), "自动检查并下载更新", |p|p.auto_update, |p,v|p.auto_update=v))
            .child(hint("自动更新默认关闭。开启后每日访问 GitHub，下载并校验 SHA256，下次启动生效。开发构建不替换自身；安装目录需可写。"))
            .child(action("检查并下载更新", false, move || check.send(Command::CheckUpdate)));
    } else {
        panel = panel
            .child(hint(
                "安卓请前往 GitHub 发布页手动下载安装更新；应用内不会自动替换 APK。",
            ))
            .child(action("打开更新下载页", false, {
                let ui = ui.clone();
                move || {
                    let ui = ui.clone();
                    spawn(async move {
                        if !open_url("https://github.com/ca-x/yank/releases").await {
                            ui.notice.set("无法打开发布页，请检查系统浏览器".into());
                        }
                    });
                }
            }));
    }
    panel.child(form_feedback(ui))
}

fn shortcut_preferences(ui: Ui) -> impl IntoElement {
    let mut fields = view().gap_3();
    for (label, save_label, get, set) in [
        (
            "显示面板快捷键",
            "保存显示面板快捷键",
            (|p: &Preferences| p.hotkey_show.clone()) as fn(&Preferences) -> String,
            (|p: &mut Preferences, value: &str| p.hotkey_show = value.into())
                as fn(&mut Preferences, &str),
        ),
        (
            "队列下一条快捷键",
            "保存队列下一条快捷键",
            |p: &Preferences| p.hotkey_next.clone(),
            |p: &mut Preferences, value: &str| p.hotkey_next = value.into(),
        ),
        (
            "暂停记录快捷键",
            "保存暂停记录快捷键",
            |p: &Preferences| p.hotkey_pause.clone(),
            |p: &mut Preferences, value: &str| p.hotkey_pause = value.into(),
        ),
        (
            "悬浮卡片快捷键",
            "保存悬浮卡片快捷键",
            |p: &Preferences| p.hotkey_float.clone(),
            |p: &mut Preferences, value: &str| p.hotkey_float = value.into(),
        ),
    ] {
        fields = fields.child(pref_text(ui.clone(), label, save_label, get, set));
    }
    fields.child(hint(
        "快捷键可留空以关闭；例如 mod+shift+v。各项修改后点击对应保存按钮。",
    ))
}

fn recovery_controls(ui: Ui) -> impl IntoElement {
    let pending = ui.clone();
    when(
        move || pending.recovery_pending.get(),
        move || {
            let retry = ui.clone();
            let dismiss = ui.clone();
            let disabled = ui.clone();
            view()
                .gap_2()
                .child(hint(
                    "自动备份尚未恢复：自动采集与备份保持暂停。原备份仍保留。",
                ))
                .child(
                    action("重试恢复自动备份", false, move || {
                        if let Some(path) = retry.recovery_path.get() {
                            retry.send(Command::Import(path));
                        }
                    })
                    .disabled(move || disabled.pending() || disabled.recovery_path.get().is_none()),
                )
                .child(action(
                    "放弃恢复，使用空资料库",
                    false,
                    move || {
                        dismiss
                            .worker
                            .shared
                            .recovery_pending
                            .store(false, Ordering::Relaxed);
                        dismiss.recovery_pending.set(false);
                    },
                ))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RevisionHarness {
        worker: std::sync::Mutex<Option<Worker>>,
    }

    #[component]
    fn RevisionControls() -> impl IntoElement {
        let harness = nsh_core::element::engine_context::<RevisionHarness>().unwrap();
        let ui = Ui::new(Rc::new(harness.worker.lock().unwrap().take().unwrap()));
        let login = ui.clone();
        let guest = ui.clone();
        let logout = ui.clone();
        let unpair = ui.clone();
        let pair = ui.clone();
        view()
            .child(action("login", false, move || {
                login.send(Command::Login(
                    "http://localhost".into(),
                    "user".into(),
                    "password".into(),
                    0,
                ))
            }))
            .child(action("guest", false, move || {
                let mut p = guest.prefs.get();
                p.detach_account();
                guest.save(p);
            }))
            .child(action("logout", false, move || {
                logout.send(Command::Logout)
            }))
            .child(action("save", false, move || {
                ui.send(Command::Save(ui.prefs.get(), 0))
            }))
            .child(action("unpair", false, move || {
                unpair.send(Command::Unpair("paired-device".into()))
            }))
            .child(action("pair", false, move || {
                pair.send(Command::PairCode("DEVICE".into(), false, 0))
            }))
    }

    #[component]
    fn QuickPasteControls() -> impl IntoElement {
        let harness = nsh_core::element::engine_context::<RevisionHarness>().unwrap();
        let ui = Ui::new(Rc::new(harness.worker.lock().unwrap().take().unwrap()));
        let clips = (1..=12)
            .map(|index| {
                let mut clip = Clip::text("test", &format!("Record {index}"));
                clip.id = format!("record-{index}");
                clip
            })
            .collect::<Vec<_>>();
        ui.snapshot.set(Some(Snapshot {
            network_status: String::new(),
            identity: None,
            discovered: Vec::new(),
            servers: Vec::new(),
            clips,
            stats: Default::default(),
            prefs: Preferences::default(),
            peers: Vec::new(),
            conflicts: Vec::new(),
            query: String::new(),
            filter: "all".into(),
            error: String::new(),
            has_more: false,
            views: Vec::new(),
            boards: Vec::new(),
            selected: None,
        }));
        let page = ui.clone();
        let content = ui.clone();
        view()
            .flex_1()
            .min_h_0()
            .child(text(move || format!("Page: {}", page.page.get())))
            .child(dynamic(move || {
                if content.page.get() == "detail" {
                    details(content.clone()).into_element()
                } else {
                    history(content.clone(), computed(|| false)).into_element()
                }
            }))
            .child(field("测试详情编辑器", || "draft".to_owned(), |_| {}).multiline(true))
            .child(action("列表外详情", false, move || {
                ui.snapshot.update(|snapshot| {
                    let snapshot = snapshot.as_mut().unwrap();
                    snapshot.selected = snapshot.clips.first().cloned();
                    snapshot.clips.clear();
                });
                ui.selected.set("record-1".into());
                ui.page.set("detail".into());
            }))
    }

    fn quick_paste_app(
        os: nsh_core::platform::Os,
    ) -> (
        nsh_testing::Headless,
        std::sync::mpsc::Receiver<Command>,
        tempfile::TempDir,
    ) {
        use std::sync::{Mutex, mpsc};
        let directory = tempfile::tempdir().unwrap();
        let running = Worker::start(directory.path().into(), false).unwrap();
        let shared = running.shared.clone();
        shared.stop.store(true, Ordering::Relaxed);
        drop(running);
        let (commands, received) = mpsc::channel();
        let (_snapshots, snapshots) = mpsc::channel();
        let harness = RevisionHarness {
            worker: Mutex::new(Some(Worker {
                commands,
                snapshots,
                shared,
            })),
        };
        let mut config =
            nsh_engine::EngineConfig::new(nsh_engine::Root::of::<QuickPasteControls>());
        config.context = config.context.with(harness);
        config.environment.os = os;
        config.environment.idiom = nsh_core::platform::Idiom::Desktop;
        let mut app = nsh_testing::Headless::start(config).unwrap();
        app.first_render().unwrap();
        // Headless does not apply native AutoFocus. Explicitly put focus in
        // search so these tests exercise editing-aware key dispatch.
        app.type_text("搜索文字、标题或收藏板", "").unwrap();
        for command in received.try_iter() {
            assert!(matches!(command, Command::Search(..)));
        }
        (app, received, directory)
    }

    #[test]
    fn search_arrows_stay_in_history_and_enter_pastes_without_detail_hydration()
    -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::Linux);
        app.press_key("arrowdown")?;
        assert!(matches!(commands.try_recv().unwrap(), Command::Select(id) if id == "record-1"));
        app.press_key("arrowdown")?;
        assert!(matches!(commands.try_recv().unwrap(), Command::Select(id) if id == "record-2"));
        assert!(
            app.shows("Page: history"),
            "keyboard selection must not open detail on narrow windows"
        );
        app.press_key("enter")?;
        assert!(matches!(commands.try_recv().unwrap(), Command::Paste(id) if id == "record-2"));
        assert!(
            commands.try_recv().is_err(),
            "one Enter must send one paste"
        );
        Ok(())
    }

    #[test]
    fn first_ten_shortcuts_use_the_platform_modifier_even_in_search() -> Result<(), String> {
        for os in [
            nsh_core::platform::Os::Linux,
            nsh_core::platform::Os::MacOs,
            nsh_core::platform::Os::Windows,
        ] {
            let (mut app, commands, _directory) = quick_paste_app(os);
            app.press_key("mod+0")?;
            assert!(
                matches!(commands.try_recv().unwrap(), Command::Paste(id) if id == "record-10")
            );
            assert!(commands.try_recv().is_err());
        }
        Ok(())
    }

    #[test]
    fn saved_detail_can_paste_after_it_leaves_the_current_result_set() -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::Linux);
        app.press("列表外详情")?;
        app.wait_for("内容详情")?;
        app.press("粘贴到原应用")?;
        assert!(matches!(commands.try_recv().unwrap(), Command::Paste(id) if id == "record-1"));
        assert!(commands.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn enter_on_a_toolbar_button_activates_it_instead_of_pasting() -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::MacOs);
        app.press_key("tab")?;
        app.press_key("enter")?;
        assert!(
            commands.try_recv().is_err(),
            "Enter on a toolbar button must not paste"
        );
        assert!(
            app.shows("Page: smart"),
            "Enter must activate the focused save-filter button"
        );
        Ok(())
    }

    #[test]
    fn enter_on_a_history_row_pastes_the_focused_record() -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::MacOs);
        app.press_key("tab")?; // Save-filter toolbar button.
        app.press_key("tab")?; // First history row.
        assert!(matches!(commands.try_recv().unwrap(), Command::Select(id) if id == "record-1"));
        app.press_key("tab")?; // Second history row.
        assert!(matches!(commands.try_recv().unwrap(), Command::Select(id) if id == "record-2"));
        app.press_key("enter")?;
        assert!(matches!(commands.try_recv().unwrap(), Command::Paste(id) if id == "record-2"));
        assert!(commands.try_recv().is_err());
        Ok(())
    }

    #[test]
    fn detail_editor_keeps_its_arrow_keys_enter_and_number_shortcuts() -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::Linux);
        app.type_text("测试详情编辑器", "draft")?;
        app.press_key("arrowdown")?;
        app.press_key("mod+1")?;
        app.press_key("enter")?;
        assert!(
            commands.try_recv().is_err(),
            "editing a draft must not select or paste history"
        );
        Ok(())
    }

    #[test]
    fn enter_during_pending_search_cannot_paste_previous_results() -> Result<(), String> {
        let (mut app, commands, _directory) = quick_paste_app(nsh_core::platform::Os::Linux);
        app.type_text("搜索文字、标题或收藏板", "new query")?;
        while let Ok(command) = commands.try_recv() {
            assert!(matches!(command, Command::Search(..)));
        }
        app.press_key("enter")?;
        assert!(
            commands.try_recv().is_err(),
            "pending search must not paste a stale row"
        );
        Ok(())
    }

    #[test]
    fn queued_commands_preserve_auth_revisions_and_revoke_before_worker_runs() -> Result<(), String>
    {
        use std::sync::{Mutex, mpsc};
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let running = Worker::start(directory.path().into(), false).map_err(|e| e.to_string())?;
        let shared = running.shared.clone();
        shared.stop.store(true, Ordering::Relaxed);
        drop(running);
        let (commands, received) = mpsc::channel();
        let (_snapshots, snapshots) = mpsc::channel();
        let harness = RevisionHarness {
            worker: Mutex::new(Some(Worker {
                commands,
                snapshots,
                shared: shared.clone(),
            })),
        };
        let mut config = nsh_engine::EngineConfig::new(nsh_engine::Root::of::<RevisionControls>());
        config.context = config.context.with(harness);
        let mut app = nsh_testing::Headless::start(config).map_err(|e| e.to_string())?;
        app.wait_for("save")?;
        app.press("save")?;
        assert!(matches!(received.try_recv().unwrap(), Command::Save(_, 0)));
        app.press("login")?;
        assert!(matches!(
            received.try_recv().unwrap(),
            Command::Login(_, _, _, 1)
        ));
        app.press("login")?;
        assert_eq!(
            shared.auth_revision.load(Ordering::SeqCst),
            1,
            "a blocked duplicate must not cancel the first login"
        );
        assert!(received.try_recv().is_err());
        app.press("guest")?;
        assert_eq!(shared.auth_revision.load(Ordering::SeqCst), 2);
        assert!(
            matches!(received.try_recv().unwrap(), Command::Save(_, 2)),
            "save uses the revision assigned while revoking the pending login"
        );
        assert!(shared.prefs.lock().unwrap().token.is_empty());
        app.press("logout")?;
        assert_eq!(shared.auth_revision.load(Ordering::SeqCst), 3);
        assert!(matches!(received.try_recv().unwrap(), Command::Logout));
        app.press("pair")?;
        assert!(matches!(
            received.try_recv().unwrap(),
            Command::PairCode(_, false, 0)
        ));
        app.press("unpair")?;
        assert!(
            shared
                .peer_access
                .lock()
                .unwrap()
                .contains_key("paired-device"),
            "unpair must revoke immediately while the worker has not processed the queue"
        );
        assert!(matches!(
            received.try_recv().unwrap(),
            Command::Unpair(id) if id == "paired-device"
        ));
        assert_eq!(shared.pair_epoch.load(Ordering::SeqCst), 1);
        app.press("pair")?;
        assert!(
            matches!(received.try_recv().unwrap(), Command::PairCode(_, false, 1)),
            "a new pairing intent must use the epoch after unpair"
        );
        Ok(())
    }

    #[test]
    fn queued_preferences_revoke_upload_and_account_without_enabling_permissions() {
        let mut shared = Preferences {
            upload: true,
            token: "session".into(),
            account_id: "account".into(),
            ..Default::default()
        };
        let mut proposed = shared.clone();
        proposed.upload = false;
        reduce_permissions(&mut shared, &proposed);
        assert!(!shared.upload);
        assert_eq!(shared.token, "session");
        proposed.upload = true;
        reduce_permissions(&mut shared, &proposed);
        assert!(
            !shared.upload,
            "an unvalidated save must never grant upload"
        );
        proposed.detach_account();
        reduce_permissions(&mut shared, &proposed);
        assert!(shared.token.is_empty());
        assert!(shared.account_id.is_empty());
        assert!(!shared.upload);
    }

    #[test]
    fn text_fallback_does_not_make_native_binary_formats_editable() {
        let mut clip = Clip::text("test", "fallback");
        assert!(text_editable(&clip));
        clip.formats.push(ClipFormat {
            format: "native-binary".into(),
            mime: None,
            data: vec![0, 255, 1],
        });
        assert!(!text_editable(&clip));
    }
}
