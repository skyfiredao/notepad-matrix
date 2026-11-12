use super::*;

struct IconTooltip {
    text: gpui::SharedString,
    dark: bool,
}

impl Render for IconTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let text = self.text.clone();
        div()
            .id("icon-tooltip")
            .debug_selector(move || format!("icon-tooltip-{text}"))
            .role(Role::Tooltip)
            .aria_label(self.text.clone())
            .px(px(6.))
            .py(px(3.))
            .border_1()
            .border_color(rgb(if self.dark { 0x808080 } else { 0x767676 }))
            .rounded(px(2.))
            .bg(rgb(if self.dark { 0x303030 } else { 0xffffe1 }))
            .text_color(rgb(if self.dark { 0xeeeeee } else { 0x202020 }))
            .font_family("Helvetica")
            .text_size(px(12.))
            .child(self.text.clone())
    }
}

pub(super) fn view(text: impl Into<gpui::SharedString>, dark: bool, cx: &mut App) -> gpui::AnyView {
    let text = text.into();
    cx.new(|_| IconTooltip { text, dark }).into()
}
