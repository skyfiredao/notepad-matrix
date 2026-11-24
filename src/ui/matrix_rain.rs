use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, App, Bounds, ContentMask, IntoElement, Pixels, ShapedLine, Styled,
    TextRun, Window, canvas, fill, font, point, px, rgb,
};

const MAX_COLUMNS: usize = 256;
const MAX_ROWS: usize = 128;
const ALPHABET: &[char] = &[
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    'A', 'B', 'C', 'D', 'E', 'F', 'H', 'I', 'J', 'K', 'L', 'M',
    'N', 'P', 'R', 'S', 'T', 'U', 'V', 'X', 'Y', 'Z', ':', '+', '=',
    'ｦ', 'ｧ', 'ｨ', 'ｩ', 'ｪ', 'ｫ', 'ｬ', 'ｭ', 'ｮ', 'ｯ', 'ｰ',
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ',
    'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ', 'ﾄ',
    'ﾅ', 'ﾆ', 'ﾇ', 'ﾈ', 'ﾉ', 'ﾊ', 'ﾋ', 'ﾌ', 'ﾍ', 'ﾎ',
    'ﾏ', 'ﾐ', 'ﾑ', 'ﾒ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ', 'ﾗ', 'ﾘ',
    'ﾙ', 'ﾚ', 'ﾛ', 'ﾜ', 'ﾝ',
];

pub(super) struct Rain {
    state: Rc<RefCell<State>>,
}

impl Rain {
    pub(super) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(State {
                random: VisualRandom(0x4d41_5452),
                streams: Vec::new(),
                rows: Vec::new(),
                glyphs: vec![None; ALPHABET.len() * 3],
                blank: None,
                columns: 0,
                scale: 1.,
                last_tick: None,
                frame_task: None,
            })),
        }
    }

    pub(super) fn render(&mut self, _window: &mut Window) -> AnyElement {
        let state = Rc::clone(&self.state);
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                state.borrow_mut().paint(bounds, window, cx);
            },
        )
        .size_full()
        .into_any_element()
    }
}

struct State {
    random: VisualRandom,
    streams: Vec<[Stream; 2]>,
    rows: Vec<CachedRow>,
    glyphs: Vec<Option<ShapedLine>>,
    blank: Option<ShapedLine>,
    columns: usize,
    scale: f32,
    last_tick: Option<Instant>,
    frame_task: Option<gpui::Task<()>>,
}

struct Stream {
    head: f32,
    length: usize,
    speed: f32,
    seed: u32,
}

impl Stream {
    fn new(rows: usize, random: &mut VisualRandom) -> Self {
        Self {
            head: -((3 + random.below(rows.max(4))) as f32),
            length: 3 + random.below(rows.clamp(6, 64) - 2),
            speed: 6. + random.below(141) as f32 / 10.,
            seed: random.next(),
        }
    }

    fn cell(&self, row: usize) -> Cell {
        let distance = self.head.floor() as i32 - row as i32;
        if distance < 0 || distance as usize >= self.length {
            return Cell::EMPTY;
        }
        let value = self.seed.wrapping_add((row as u32).wrapping_mul(0x9e37_79b9));
        let value = (value ^ (value >> 16)).wrapping_mul(0x85eb_ca6b);
        let value = value ^ (value >> 13);
        Cell {
            glyph: value as usize % ALPHABET.len(),
            tone: if distance == 0 {
                3
            } else if distance <= 2 || value % 11 == 0 {
                2
            } else {
                1
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    glyph: usize,
    tone: u8,
}

impl Cell {
    const EMPTY: Self = Self { glyph: 0, tone: 0 };
}

struct CachedRow {
    cells: Vec<Cell>,
    dirty: bool,
}

struct VisualRandom(u32);

impl VisualRandom {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    fn below(&mut self, upper: usize) -> usize {
        self.next() as usize % upper
    }
}

impl State {
    fn resize(&mut self, bounds: Bounds<Pixels>) -> bool {
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        let scale = (width / (MAX_COLUMNS as f32 * 8.))
            .max(height / (MAX_ROWS as f32 * 16.))
            .max(1.);
        let columns = ((width / (8. * scale)).ceil() as usize).clamp(1, MAX_COLUMNS);
        let rows = ((height / (16. * scale)).ceil() as usize).clamp(1, MAX_ROWS);
        let resized = columns != self.columns || rows != self.rows.len();
        if resized {
            self.columns = columns;
            self.streams = (0..columns.div_ceil(2))
                .map(|_| std::array::from_fn(|_| {
                    let mut stream = Stream::new(rows, &mut self.random);
                    stream.head = self.random.below(rows + stream.length) as f32
                        - stream.length as f32;
                    stream
                }))
                .collect();
            self.rows = (0..rows)
                .map(|_| CachedRow {
                    cells: vec![Cell::EMPTY; columns],
                    dirty: true,
                })
                .collect();
            self.last_tick = None;
        }
        if self.scale != scale {
            self.blank = None;
            self.glyphs.fill(None);
        }
        self.scale = scale;
        resized
    }

    fn advance(&mut self, now: Instant, running: bool) -> bool {
        if !running {
            self.last_tick = None;
            return false;
        }
        let Some(last_tick) = self.last_tick else {
            self.last_tick = Some(now);
            return true;
        };
        let elapsed = now.saturating_duration_since(last_tick);
        if elapsed < Duration::from_millis(33) {
            return false;
        }
        self.last_tick = Some(now);
        if elapsed > Duration::from_millis(250) {
            return false;
        }
        let delta = elapsed.as_secs_f32().min(0.1);
        for streams in &mut self.streams {
            for stream in streams {
                stream.head += stream.speed * delta;
                if stream.head - stream.length as f32 >= self.rows.len() as f32 {
                    *stream = Stream::new(self.rows.len(), &mut self.random);
                }
            }
        }
        true
    }

    fn update_row(&mut self, index: usize) {
        let row = &mut self.rows[index];
        if !row.dirty {
            return;
        }
        row.dirty = false;
        for (column, cell) in row.cells.iter_mut().enumerate() {
            let mut next = Cell::EMPTY;
            if column % 2 == 0 {
                for stream in &self.streams[column / 2] {
                    let candidate = stream.cell(index);
                    if candidate.tone > next.tone {
                        next = candidate;
                    }
                }
            }
            *cell = next;
        }
    }

    fn shape_glyph(&self, glyph: char, tone: u8, window: &Window) -> ShapedLine {
        let text = glyph.to_string();
        let run = TextRun {
            len: text.len(), font: font("Menlo"),
            color: rgb(match tone { 3 => 0xffffff, 2 => 0x00ff00, 1 => 0x00aa00, _ => 0x000000 }).into(),
            background_color: None, underline: None, strikethrough: None,
        };
        window.text_system().shape_line(text.into(), px(13. * self.scale), &[run], Some(px(8. * self.scale)))
    }

    fn paint(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let visible = bounds.intersect(&window.content_mask().bounds)
            .intersect(&window.fully_visible_bounds());
        if visible.size.width <= px(0.) || visible.size.height <= px(0.)
            || !window.is_visible()
        {
            self.last_tick = None;
            self.frame_task = None;
            return;
        }
        let resized = self.resize(bounds);
        let running = super::matrix_surface::animation_running(window, cx);
        let update = self.advance(Instant::now(), running) || resized;
        if update {
            for row in &mut self.rows {
                row.dirty = true;
            }
        }
        let line_height = px(16. * self.scale);
        let first = (f32::from(visible.top() - bounds.top()) / f32::from(line_height))
            .floor().max(0.) as usize;
        let end = ((f32::from(visible.bottom() - bounds.top()) / f32::from(line_height))
            .ceil() as usize).min(self.rows.len());
        if self.blank.is_none() { self.blank = Some(self.shape_glyph(' ', 0, window)); }
        window.with_content_mask(Some(ContentMask { bounds: visible }), |window| {
            window.paint_quad(fill(visible, rgb(0x000000)));
            for index in first..end {
                self.update_row(index);
                let Some(blank) = &self.blank else { continue; };
                let (mut ascent, mut descent) = (blank.ascent, blank.descent);
                for column in 0..self.columns {
                    let cell = self.rows[index].cells[column];
                    if cell.tone == 0 { continue; }
                    let key = cell.glyph * 3 + usize::from(cell.tone - 1);
                    if self.glyphs[key].is_none() {
                        self.glyphs[key] = Some(self.shape_glyph(ALPHABET[cell.glyph], cell.tone, window));
                    }
                    if let Some(line) = &self.glyphs[key] {
                        ascent = ascent.max(line.ascent);
                        descent = descent.max(line.descent);
                    }
                }
                let baseline = (line_height - ascent - descent) / 2. + ascent;
                for (column, cell) in self.rows[index].cells.iter().enumerate() {
                    if cell.tone == 0 { continue; }
                    let key = cell.glyph * 3 + usize::from(cell.tone - 1);
                    let Some(line) = &self.glyphs[key] else { continue; };
                    let glyph_baseline = (line_height - line.ascent - line.descent) / 2. + line.ascent;
                    let origin = point(bounds.left() + px(8. * self.scale * column as f32),
                        bounds.top() + line_height * index as f32 + baseline - glyph_baseline);
                    let color = rgb(match cell.tone { 3 => 0xffffff, 2 => 0x00ff00, _ => 0x00aa00 }).into();
                    if let Err(error) = super::matrix_surface::paint_animation_line(line, origin, line_height, color, None, window) {
                        eprintln!("Matrix rain paint failed: {error}");
                        self.last_tick = None;
                        return;
                    }
                }
            }
        });
        super::matrix_surface::schedule_animation_frame(self.last_tick, Duration::from_millis(33),
            &mut self.frame_task, running, window, cx);
    }
}
