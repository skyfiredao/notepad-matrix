#[derive(Clone, Copy, Debug)]
pub struct SelectionDrag {
    x: f32,
    y: f32,
    active: bool,
    target: usize,
}

impl SelectionDrag {
    pub fn new(x: f32, y: f32, byte: usize) -> Self {
        Self {
            x,
            y,
            active: false,
            target: byte,
        }
    }

    pub fn update(&mut self, x: f32, y: f32, byte: usize) {
        self.active |= (x - self.x).hypot(y - self.y) >= 3.;
        self.target = byte;
    }

    pub fn drop_target(self) -> Option<usize> {
        self.active.then_some(self.target)
    }
}
