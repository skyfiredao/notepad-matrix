use super::*;
use objc2_foundation::{NSDate, NSDateFormatter, NSDateFormatterStyle};
use std::time::{SystemTime, UNIX_EPOCH};

fn format_file_time(time: SystemTime) -> String {
    let seconds = match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    };
    let date = NSDate::dateWithTimeIntervalSince1970(seconds);
    let formatter = NSDateFormatter::new();
    formatter.setDateStyle(NSDateFormatterStyle::ShortStyle);
    formatter.setTimeStyle(NSDateFormatterStyle::MediumStyle);
    formatter.stringFromDate(&date).to_string()
}

impl MatrixWindow {
    pub(super) fn show_summary(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        self.summary_visible = true;
        self.present_floating(floating_windows::DialogKind::Summary, window, cx);
        cx.notify();
    }

    pub(super) fn render_summary(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.summary_visible {
            return None;
        }
        let snapshot = self.editor().display_snapshot();
        let mut description = String::new();
        if let Some(file) = self.disk_file()
            && let Ok(metadata) = std::fs::metadata(file.path())
        {
            description.push_str(&format!(
                "Full file path: {}\nCreated: {}\nModified: {}\n",
                file.path().display(),
                metadata
                    .created()
                    .ok()
                    .map(format_file_time)
                    .unwrap_or_default(),
                metadata
                    .modified()
                    .ok()
                    .map(format_file_time)
                    .unwrap_or_default(),
            ));
        }
        match crate::status::summarize(
            &snapshot,
            self.editor().selection(),
            self.workspace.active().format(),
            self.editor().line_ending(),
        ) {
            Ok(summary) => description.push_str(&summary.description()),
            Err(error) => description.push_str(&format!("Summary unavailable: {error}")),
        }
        Some(
            div()
                .id("summary-dialog")
                .debug_selector(|| "summary-dialog".into())
                .role(Role::Dialog)
                .aria_label("Summary")
                .track_focus(&self.close_focus)
                .w_full()
                .p(px(16.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(description)
                .child(
                    div()
                        .id("summary-close")
                        .debug_selector(|| "summary-close".into())
                        .role(Role::Button)
                        .aria_label("Close Summary")
                        .border_1()
                        .border_color(rgb(0x808080))
                        .px(px(12.))
                        .py(px(4.))
                        .child("OK")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.dismiss_floating(
                                floating_windows::DialogKind::Summary,
                                window,
                                cx,
                            );
                        })),
                )
                .into_any_element(),
        )
    }
}
