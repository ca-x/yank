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
        .hover(|s| s.opacity(0.88))
        .active(|s| s.opacity(0.65))
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
    svg(ImageSrc::svg_markup(
        yank_core::icons::svg(name).unwrap_or_default(),
    ))
    .w(px(28.0))
    .h(px(28.0))
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
    let border = selected.clone();
    styled_action(false, run)
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
        .border_color(move || {
            if border() {
                theme().primary
            } else {
                theme().secondary
            }
        })
        .selected(move || selected())
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
    }
}

pub fn toggle(
    label: &'static str,
    checked: impl Fn() -> bool + 'static,
    change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    switch()
        .checked(checked)
        .on_change(move |&value: &bool| change(value))
        .flex_row()
        .items_center()
        .justify_between()
        .gap_4()
        .min_h(px(48.0))
        .track_class("w-11 rounded-full bg-muted p-0.5 checked:bg-primary")
        .thumb_class("size-5 rounded-full bg-card checked:translate-x-5")
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .child(
            text(crate::l10n::label(label))
                .text_sm()
                .text_color(move || theme().foreground),
        )
}
pub fn surface() -> nsh_core::element::View {
    view()
        .p_4()
        .gap_3()
        .rounded(move || theme().radius)
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
        .text_sm()
        .text_color(move || theme().muted_foreground)
}

pub struct Field {
    label: &'static str,
    control: Input,
    action: Option<ButtonElement>,
}
impl Field {
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
    pub fn action(mut self, action: ButtonElement) -> Self {
        self.action = Some(action);
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
        let mut content = view()
            .gap_1()
            .min_w_0()
            .child(
                text(crate::l10n::label(self.label))
                    .text_sm()
                    .text_color(move || theme().muted_foreground),
            )
            .child(self.control);
        if let Some(action) = self.action {
            content = content.child(action);
        }
        content.into_element()
    }
}
