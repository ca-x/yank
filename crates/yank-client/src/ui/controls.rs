//! App-owned styled NSH controls shared by every screen.
use nsh_base::{ButtonElement, Input, button, input, switch, theme};
use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;
pub fn action(
    label: impl IntoReactive<SharedString>,
    primary: bool,
    run: impl Fn() + 'static,
) -> ButtonElement {
    styled_action(primary, run).child(action_label(label, primary))
}

fn styled_action(primary: bool, run: impl Fn() + 'static) -> ButtonElement {
    button()
        .on_press(run)
        .min_h(move || px(if is_desktop() { 36.0 } else { 48.0 }))
        .px_3()
        .py_2()
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || {
            if primary {
                theme().primary
            } else {
                theme().border
            }
        })
        .bg(move || {
            if primary {
                theme().primary
            } else {
                theme().card
            }
        })
        .hover(move |style| {
            style.bg(if primary {
                theme().primary.opacity(0.9)
            } else {
                theme().muted
            })
        })
        .active(move |style| {
            style.bg(if primary {
                theme().primary.opacity(0.8)
            } else {
                theme().accent
            })
        })
        .transition_colors()
        .duration(move || if reduce_motion() { 0.0_f32 } else { 120.0_f32 })
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .disabled_style(|s| s.opacity(0.4))
        .flex_row()
        .gap_2()
        .items_center()
        .justify_center()
}

fn action_label(label: impl IntoReactive<SharedString>, primary: bool) -> nsh_core::element::Text {
    text(crate::l10n::label(label))
        .text_sm()
        .font_weight(FontWeight::MEDIUM)
        .text_color(move || {
            if primary {
                theme().primary_foreground
            } else {
                theme().foreground
            }
        })
}

pub fn icon(name: &str) -> impl IntoElement {
    let (name, angle) = match name {
        "arrow-up" => ("arrow-right", -90.0),
        "arrow-down" => ("arrow-right", 90.0),
        _ => (name, 0.0),
    };
    svg(ImageSrc::svg_markup(
        yank_core::icons::svg(name).unwrap_or_default(),
    ))
    .w(px(18.0))
    .h(px(18.0))
    .rotate(deg(angle))
    .flex_shrink_0()
    .accessible(false)
}

pub fn icon_action(
    label: impl IntoReactive<SharedString>,
    name: &str,
    primary: bool,
    run: impl Fn() + 'static,
) -> ButtonElement {
    styled_action(primary, run)
        .child(icon(name))
        .child(action_label(label, primary))
}

pub fn nav_action(
    label: &'static str,
    name: &str,
    selected: impl Fn() -> bool + 'static,
    run: impl Fn() + 'static,
) -> ButtonElement {
    let selected = std::rc::Rc::new(selected);
    let background = selected.clone();
    let indicator = selected.clone();
    button()
        .on_press(run)
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(36.0))
        .px_2()
        .rounded(move || theme().radius)
        .border_l(px(3.0))
        .border_color(move || {
            if indicator() {
                theme().primary
            } else {
                theme().secondary
            }
        })
        .hover(|style| style.bg(theme().muted))
        .active(|style| style.bg(theme().accent))
        .focus(|style| style.outline(px(2.0)).outline_color(theme().ring))
        .child(icon(name))
        .child(action_label(label, false).flex_1().min_w_0().truncate())
        .min_w_0()
        .justify_start()
        .bg(move || {
            if background() {
                theme().accent
            } else {
                theme().secondary
            }
        })
        .selected(move || selected())
}

pub fn keycap(label: impl IntoReactive<SharedString>) -> impl IntoElement {
    text(label)
        .text_xs()
        .px_1()
        .py(px(2.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().secondary)
        .text_color(move || theme().muted_foreground)
}

pub fn quiet_action(label: &'static str, run: impl Fn() + 'static) -> ButtonElement {
    button()
        .on_press(run)
        .min_h(move || px(if is_desktop() { 32.0 } else { 44.0 }))
        .px_2()
        .rounded(move || theme().radius)
        .hover(|style| style.bg(theme().accent))
        .active(|style| style.bg(theme().accent))
        .focus(|style| style.outline(px(2.0)).outline_color(theme().ring))
        .child(action_label(label, false))
}

pub fn tool(
    label: impl IntoReactive<SharedString>,
    name: &str,
    run: impl Fn() + 'static,
) -> impl IntoElement {
    let label = crate::l10n::label(label);
    let accessible_label = label.clone();
    let destructive = name == "trash";
    view()
        .tooltip(
            nsh_base::tooltip()
                .px_2()
                .py_1()
                .rounded(px(6.0))
                .bg(move || theme().foreground)
                .child(text(label).text_xs().text_color(move || theme().card)),
        )
        .child(
            button()
                .on_press(run)
                .w(move || px(if is_desktop() { 32.0 } else { 44.0 }))
                .h(move || px(if is_desktop() { 32.0 } else { 44.0 }))
                .items_center()
                .justify_center()
                .rounded(move || theme().radius)
                .accessibility_label(move || Some(accessible_label.get()))
                .bg(move || {
                    if destructive {
                        theme().destructive.opacity(0.08)
                    } else {
                        theme().card
                    }
                })
                .hover(move |style| {
                    style.bg(if destructive {
                        theme().destructive.opacity(0.18)
                    } else {
                        theme().muted
                    })
                })
                .active(|style| style.bg(theme().accent))
                .focus(|style| style.outline(px(3.0)).outline_color(theme().ring))
                .child(icon(name)),
        )
}

pub fn menu_command(
    label: impl IntoReactive<SharedString>,
    name: &str,
    run: impl Fn() + 'static,
) -> impl IntoElement {
    let label = crate::l10n::label(label);
    let destructive = name == "trash";
    let run = std::rc::Rc::new(run);
    let name = name.to_owned();
    dynamic(move || {
        let run = run.clone();
        nsh_base::menu_item()
            .on_select(move || run())
            .min_h(px(36.0))
            .px_2()
            .flex_row()
            .gap_2()
            .items_center()
            .rounded(px(6.0))
            .hover(|style| style.bg(theme().accent))
            .focus(|style| style.bg(theme().accent))
            .child(icon(&name))
            .child(action_label(label.clone(), false).text_color(move || {
                if destructive {
                    theme().destructive
                } else {
                    theme().foreground
                }
            }))
    })
}
pub fn field(
    label: &'static str,
    value: impl IntoReactive<String>,
    changed: impl Fn(&str) + 'static,
) -> Field {
    let control = input()
        .label(move || crate::l10n::tr(label))
        .value(value)
        .on_change(changed)
        .placeholder(move || crate::l10n::tr(label))
        .min_h(move || px(if is_desktop() { 38.0 } else { 48.0 }))
        .px_3()
        .py_2()
        .rounded(move || theme().radius)
        .bg(move || theme().card)
        .border_1()
        .border_color(move || theme().input)
        .text_color(move || theme().foreground)
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .min_w_0();
    Field {
        label,
        control,
        action: None,
        show_label: true,
    }
}

pub fn toggle(
    label: &'static str,
    checked: impl Fn() -> bool + 'static,
    change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    view()
        .flex_row()
        .w_full()
        .min_w_0()
        .items_center()
        .justify_between()
        .gap_4()
        .min_h(px(44.0))
        .child(
            text(crate::l10n::label(label))
                .text_sm()
                .flex_1()
                .min_w_0()
                .text_color(move || theme().foreground),
        )
        .child(
            switch()
                .accessibility_label(move || Some(SharedString::from(crate::l10n::tr(label))))
                .checked(checked)
                .on_change(move |&value: &bool| change(value))
                .flex_row()
                .items_center()
                .min_h(px(44.0))
                .track_class("w-11 rounded-full bg-muted p-0.5 checked:bg-primary")
                .thumb_class("size-5 rounded-full bg-card checked:translate-x-5")
                .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring)),
        )
}
pub fn surface() -> nsh_core::element::View {
    view()
        .w_full()
        .min_w_0()
        .p_4()
        .gap_3()
        .rounded(move || px(theme().number("radius-card").unwrap_or(10.0)))
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
}
pub fn title(label: &'static str) -> impl IntoElement {
    text(crate::l10n::label(label))
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .role(AccessibilityRole::Header)
        .accessibility_level(2)
        .text_color(move || theme().foreground)
}
pub fn hint(label: impl IntoReactive<SharedString>) -> impl IntoElement {
    let label = label.into_reactive();
    text(move || crate::l10n::render(&label.get()))
        .max_w(relative(1.0))
        .min_w_0()
        .text_sm()
        .text_color(move || theme().muted_foreground)
}

pub struct Field {
    label: &'static str,
    control: Input,
    action: Option<ButtonElement>,
    show_label: bool,
}
impl Field {
    pub fn hide_label(mut self) -> Self {
        self.show_label = false;
        self
    }
    pub fn element_ref(mut self, handle: &ElementRef) -> Self {
        self.control = self.control.element_ref(handle);
        self
    }
    pub fn auto_focus(mut self, value: bool) -> Self {
        self.control = self.control.auto_focus(value);
        self
    }
    pub fn on_submit(mut self, submit: impl Fn(&str) + 'static) -> Self {
        self.control = self.control.on_submit(submit);
        self
    }
    pub fn masked(mut self, value: bool) -> Self {
        self.control = self.control.masked(value);
        self
    }
    pub fn multiline(mut self, value: bool) -> Self {
        self.control = self.control.multiline(value);
        self
    }
    pub fn min_h(mut self, value: Points) -> Self {
        self.control = self.control.min_h(value);
        self
    }
    pub fn readonly(mut self, value: impl Fn() -> bool + 'static) -> Self {
        self.control = self.control.readonly(value);
        self
    }
}
impl IntoElement for Field {
    fn into_element(self) -> AnyElement {
        let mut content = view().gap_1().min_w_0().w_full();
        if self.show_label {
            content = content.child(
                text(crate::l10n::label(self.label))
                    .w_full()
                    .min_w_0()
                    .text_sm()
                    .text_color(move || theme().muted_foreground),
            );
        }
        content = content.child(self.control);
        if let Some(action) = self.action {
            content = content.child(action);
        }
        content.into_element()
    }
}
