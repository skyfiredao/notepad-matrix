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
        if self.route_to_document(window, cx, |view, window, cx| view.show_summary(window, cx)) {
            return;
        }
        if self.summary_visible
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
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
        self.summary_visible = true;
        let response = window.prompt(
            gpui::PromptLevel::Info,
            "Summary",
            Some(&description),
            &[gpui::PromptButton::ok("OK")],
            cx,
        );
        cx.spawn_in(window, async move |view, cx| {
            if let Err(error) = response.await {
                eprintln!("Summary prompt closed without a response: {error}");
            }
            if let Err(error) = view.update_in(cx, |view, window, cx| {
                view.summary_visible = false;
                view.editor_focus.focus(window, cx);
                cx.notify();
            }) {
                eprintln!("Summary prompt lost its document window: {error}");
            }
        })
        .detach();
        cx.notify();
    }
}
