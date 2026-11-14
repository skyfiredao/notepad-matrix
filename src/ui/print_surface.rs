use super::*;
use objc2::MainThreadOnly;
use objc2_app_kit::{NSFont, NSPrintInfo, NSPrintOperation, NSPrintingPaginationMode, NSTextView};
use objc2_foundation::{NSCopying, NSPoint, NSRect, NSSize, NSString};
use std::path::Path;

fn print_text(
    text: &str,
    title: &str,
    family: &str,
    font_size: f64,
    show_panel: bool,
) -> io::Result<bool> {
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| io::Error::other("printing requires the main thread"))?;
    let info = NSPrintInfo::sharedPrintInfo().copy();
    info.setHorizontalPagination(NSPrintingPaginationMode::Automatic);
    info.setVerticalPagination(NSPrintingPaginationMode::Automatic);
    info.setHorizontallyCentered(false);
    info.setVerticallyCentered(false);
    let width = info.paperSize().width - info.leftMargin() - info.rightMargin();
    let view = NSTextView::initWithFrame(
        NSTextView::alloc(mtm),
        NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, 1.)),
    );
    view.setRichText(false);
    view.setEditable(false);
    view.setHorizontallyResizable(false);
    view.setVerticallyResizable(true);
    view.setMaxSize(NSSize::new(width, f64::MAX));
    view.setTextContainerInset(NSSize::new(0., 0.));
    let font = NSFont::fontWithName_size(&NSString::from_str(family), font_size)
        .ok_or_else(|| io::Error::other(format!("print font unavailable: {family}")))?;
    view.setFont(Some(&font));
    view.setString(&NSString::from_str(text));
    view.sizeToFit();
    let operation = NSPrintOperation::printOperationWithView_printInfo(&view, &info);
    let title = Path::new(title)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(title);
    operation.setJobTitle(Some(&NSString::from_str(title)));
    operation.setShowsPrintPanel(show_panel);
    operation.setShowsProgressPanel(true);
    Ok(operation.runOperation())
}

impl MatrixWindow {
    pub(super) fn print_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.print_document_mode(true, window, cx);
    }

    pub(super) fn print_document_mode(
        &mut self,
        show_panel: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.route_to_document(window, cx, move |view, window, cx| {
            view.print_document_mode(show_panel, window, cx)
        }) {
            return;
        }
        if self.printing
            || self.file_busy
            || self.close_dialog
            || self.save_all_dialog
            || self.batch_editing_blocked()
        {
            return;
        }
        self.menu_input(MenuEvent::Dismiss, window, cx);
        let text = self.editor().display_text();
        let title = self.workspace.active().name().to_string();
        let family = self.editor_font_family();
        let font_size = f64::from(f32::from(self.editor_font_size()));
        self.printing = true;
        cx.notify();
        cx.spawn(async move |view, cx| {
            let result = print_text(&text, &title, family, font_size, show_panel);
            if let Err(error) = view.update(cx, |view, cx| {
                view.printing = false;
                match result {
                    Ok(true) => {}
                    Ok(false) => eprintln!("Print operation cancelled or did not complete"),
                    Err(error) => {
                        view.notify_editor_error(format!("Could not print: {error}"), cx);
                    }
                }
                cx.notify();
            }) {
                eprintln!("printing lost its document: {error}");
            }
        })
        .detach();
    }
}
