//! App-owned styled NSH controls shared by every screen.
use nsh_base::{ButtonElement, Input, button, input, switch, theme};
use nsh_core::prelude::*;
pub fn action(
    label: impl IntoReactive<SharedString>,
    primary: bool,
    run: impl Fn() + 'static,
) -> ButtonElement {
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
        .items_center()
        .justify_center()
        .child(
            text(label)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || {
                    if primary {
                        theme().primary_foreground
                    } else {
                        theme().foreground
                    }
                }),
        )
}
pub fn field(
    label: &'static str,
    value: impl IntoReactive<String>,
    changed: impl Fn(&str) + 'static,
) -> Field {
    let control = input()
        .label(label)
        .value(value)
        .on_change(changed)
        .placeholder(label)
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
        .child(text(label).text_sm().text_color(move || theme().foreground))
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
    text(label)
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .role(AccessibilityRole::Header)
        .accessibility_level(2)
        .text_color(move || theme().foreground)
}
pub fn hint(label: impl IntoReactive<SharedString>) -> impl IntoElement {
    text(label)
        .text_sm()
        .text_color(move || theme().muted_foreground)
}

pub struct Field {
    label: &'static str,
    control: Input,
    action: Option<ButtonElement>,
}
impl Field {
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
                text(self.label)
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
