use gpui_apple::display_link::MetalFrameSource;
use std::cell::{Cell, RefCell};

#[derive(Default)]
pub struct MetalFrames {
    source: RefCell<Option<MetalFrameSource>>,
    woken: Cell<bool>,
}

impl MetalFrames {
    pub fn replace(&self, source: Option<MetalFrameSource>) {
        *self.source.borrow_mut() = source;
    }

    pub fn is_attached(&self) -> bool {
        self.source.borrow().is_some()
    }

    pub fn wake(&self) {
        self.woken.set(true);
        if let Ok(source) = self.source.try_borrow()
            && let Some(source) = source.as_ref()
        {
            source.set_paused(false);
        }
    }

    pub fn begin_frame(&self) {
        self.woken.set(false);
    }

    pub fn end_frame(&self, used_drawable: bool) -> bool {
        let pause = !used_drawable && !self.woken.get();
        if pause && let Some(source) = self.source.borrow().as_ref() {
            source.set_paused(true);
        }
        pause
    }
}
