use std::collections::BTreeSet;
use std::ops::{Range, RangeInclusive};

#[derive(Clone, Copy)]
struct Run {
    start: usize,
    end: usize,
    edition: u8,
}

#[derive(Clone)]
struct Gap<T> {
    left: Vec<T>,
    right: Vec<T>,
}

impl<T> Gap<T> {
    fn new(values: Vec<T>) -> Self {
        Self {
            left: values,
            right: Vec::new(),
        }
    }
    fn len(&self) -> usize {
        self.left.len() + self.right.len()
    }
    fn at(&self, index: usize) -> &T {
        if index < self.left.len() {
            &self.left[index]
        } else {
            &self.right[self.len() - index - 1]
        }
    }
    fn at_mut(&mut self, index: usize) -> &mut T {
        if index < self.left.len() {
            &mut self.left[index]
        } else {
            let index = self.len() - index - 1;
            &mut self.right[index]
        }
    }
    fn move_to(&mut self, index: usize) {
        while self.left.len() > index {
            self.right.push(self.left.pop().expect("source gap left"));
        }
        while self.left.len() < index {
            self.left.push(self.right.pop().expect("source gap right"));
        }
    }
    fn insert(&mut self, index: usize, value: T) {
        self.move_to(index);
        self.left.push(value);
    }
    fn remove(&mut self, index: usize) -> T {
        self.move_to(index);
        self.right.pop().expect("source gap removal")
    }
    fn set(&mut self, index: usize, value: T) -> T {
        std::mem::replace(self.at_mut(index), value)
    }
}

#[derive(Clone)]
struct Positions {
    values: Gap<isize>,
    step: isize,
    delta: isize,
}

impl Positions {
    fn new() -> Self {
        Self {
            values: Gap::new(vec![0, 0]),
            step: 0,
            delta: 0,
        }
    }
    fn count(&self) -> usize {
        self.values.len() - 1
    }
    fn at(&self, index: usize) -> usize {
        (*self.values.at(index)
            + if index as isize > self.step {
                self.delta
            } else {
                0
            }) as usize
    }
    fn length(&self) -> usize {
        self.at(self.count())
    }
    fn apply_step(&mut self, upto: usize) {
        if self.delta != 0 {
            for index in self.step + 1..=upto as isize {
                *self.values.at_mut(index as usize) += self.delta;
            }
        }
        self.step = upto as isize;
        if upto >= self.count() {
            self.step = self.count() as isize;
            self.delta = 0;
        }
    }
    fn back_step(&mut self, down: usize) {
        if self.delta != 0 {
            for index in down as isize + 1..=self.step {
                *self.values.at_mut(index as usize) -= self.delta;
            }
        }
        self.step = down as isize;
    }
    fn shift(&mut self, index: usize, delta: isize) {
        if self.delta != 0 {
            if index as isize >= self.step {
                self.apply_step(index);
                self.delta += delta;
            } else if index as isize >= self.step - self.values.len() as isize / 10 {
                self.back_step(index);
                self.delta += delta;
            } else {
                self.apply_step(self.count());
                self.step = index as isize;
                self.delta = delta;
            }
        } else {
            self.step = index as isize;
            self.delta = delta;
        }
    }
    fn insert(&mut self, index: usize, position: usize) {
        if self.step < index as isize {
            self.apply_step(index);
        }
        self.values.insert(index, position as isize);
        self.step += 1;
    }
    fn remove(&mut self, index: usize) {
        if index as isize > self.step {
            self.apply_step(index);
        }
        self.step -= 1;
        self.values.remove(index);
    }
    fn partition(&self, position: usize) -> usize {
        if position >= self.length() {
            return self.count() - 1;
        }
        let (mut lower, mut upper) = (0, self.count());
        while lower < upper {
            let middle = (upper + lower).div_ceil(2);
            if position < self.at(middle) {
                upper = middle - 1;
            } else {
                lower = middle;
            }
        }
        lower
    }
}

#[derive(Clone)]
struct Runs {
    positions: Positions,
    values: Gap<u8>,
}

impl Runs {
    fn new(length: usize) -> Self {
        let mut runs = Self {
            positions: Positions::new(),
            values: Gap::new(vec![0, 0]),
        };
        runs.insert_space(0, length);
        runs
    }
    fn length(&self) -> usize {
        self.positions.length()
    }
    fn at(&self, position: usize) -> u8 {
        *self.values.at(self.positions.partition(position))
    }
    fn end(&self, position: usize) -> usize {
        self.positions.at(self.positions.partition(position) + 1)
    }
    fn run_from(&self, position: usize) -> usize {
        let mut run = self.positions.partition(position);
        while run > 0 && position == self.positions.at(run - 1) {
            run -= 1;
        }
        run
    }
    fn split(&mut self, position: usize) -> usize {
        let mut run = self.run_from(position);
        if self.positions.at(run) < position {
            let value = self.at(position);
            run += 1;
            self.positions.insert(run, position);
            self.values.insert(run, value);
        }
        run
    }
    fn remove(&mut self, run: usize) {
        self.positions.remove(run);
        self.values.remove(run);
    }
    fn remove_empty(&mut self, run: usize) {
        if run < self.positions.count()
            && self.positions.count() > 1
            && self.positions.at(run) == self.positions.at(run + 1)
        {
            self.remove(run);
        }
    }
    fn remove_same(&mut self, run: usize) {
        if run > 0 && run < self.positions.count() && self.values.at(run - 1) == self.values.at(run)
        {
            self.remove(run);
        }
    }

    fn insert_space(&mut self, position: usize, length: usize) {
        let run = self.run_from(position);
        if self.positions.at(run) == position {
            let value = self.at(position);
            if run == 0 && value != 0 {
                self.values.set(0, 0);
                self.positions.insert(1, 0);
                self.values.insert(1, value);
                self.positions.shift(0, length as isize);
            } else {
                self.positions.shift(
                    if run > 0 && value != 0 { run - 1 } else { run },
                    length as isize,
                );
            }
        } else {
            self.positions.shift(run, length as isize);
        }
    }

    fn fill(&mut self, range: Range<usize>, edition: u8) {
        if range.is_empty() || range.end > self.length() {
            return;
        }
        let (position, mut end) = (range.start, range.end);
        let mut run_end = self.run_from(end);
        let current = *self.values.at(run_end);
        if current == edition {
            end = self.positions.at(run_end);
            if position >= end {
                return;
            }
        } else {
            if position > self.positions.at(run_end) && end < self.positions.at(run_end + 1) {
                self.positions.insert(run_end + 1, position);
                self.positions.insert(run_end + 2, end);
                self.values.insert(run_end + 1, edition);
                self.values.insert(run_end + 2, current);
                return;
            }
            run_end = self.split(end);
        }
        let mut run_start = self.run_from(position);
        if *self.values.at(run_start) == edition {
            run_start += 1;
        } else if self.positions.at(run_start) < position {
            run_start = self.split(position);
            run_end += 1;
        }
        if run_start < run_end {
            self.values.set(run_start, edition);
            for _ in run_start + 1..run_end {
                self.remove(run_start + 1);
            }
            let run_end = self.run_from(end);
            self.remove_same(run_end);
            self.remove_same(run_start);
            let run_end = self.run_from(end);
            self.remove_empty(run_end);
        }
    }

    fn delete(&mut self, range: Range<usize>) {
        let (run_start, run_end) = (self.run_from(range.start), self.run_from(range.end));
        if run_start == run_end {
            self.positions.shift(run_start, -(range.len() as isize));
            self.remove_empty(run_start);
        } else {
            let (run_start, run_end) = (self.split(range.start), self.split(range.end));
            self.positions.shift(run_start, -(range.len() as isize));
            for _ in run_start..run_end {
                self.remove(run_start);
            }
            self.remove_empty(run_start);
            self.remove_same(run_start);
        }
    }
}

#[derive(Clone, Copy)]
struct EditionCount {
    edition: u8,
    count: usize,
}

#[derive(Clone)]
struct Deletions {
    positions: Positions,
    values: Gap<Option<Vec<EditionCount>>>,
}

impl Deletions {
    fn new(length: usize) -> Self {
        let mut deletions = Self {
            positions: Positions::new(),
            values: Gap::new(vec![None, None]),
        };
        deletions.insert_space(0, length);
        deletions
    }
    fn element(&self, position: usize) -> usize {
        if position < self.positions.length() {
            self.positions.partition(position)
        } else {
            self.positions.count()
        }
    }
    fn get(&self, position: &usize) -> Option<&Vec<EditionCount>> {
        let index = self.element(*position);
        if self.positions.at(index) == *position {
            self.values.at(index).as_ref()
        } else {
            None
        }
    }
    fn get_mut(&mut self, position: &usize) -> Option<&mut Vec<EditionCount>> {
        let index = self.element(*position);
        if self.positions.at(index) == *position {
            self.values.at_mut(index).as_mut()
        } else {
            None
        }
    }
    fn contains_key(&self, position: &usize) -> bool {
        self.get(position).is_some()
    }
    fn insert(&mut self, position: usize, values: Vec<EditionCount>) {
        let index = self.element(position);
        if self.positions.at(index) == position {
            self.values.set(index, Some(values));
        } else {
            self.positions.insert(index + 1, position);
            self.values.insert(index + 1, Some(values));
        }
    }
    fn remove(&mut self, position: &usize) -> Option<Vec<EditionCount>> {
        self.get(position)?;
        let index = self.element(*position);
        let values = self.values.set(index, None);
        if index > 0 && index < self.positions.count() {
            self.positions.remove(index);
            self.values.remove(index);
        }
        values
    }
    fn range(
        &self,
        range: RangeInclusive<usize>,
    ) -> impl Iterator<Item = (usize, &Vec<EditionCount>)> {
        let (start, end) = (*range.start(), *range.end());
        (self.element(start)..=self.positions.count())
            .take_while(move |&index| self.positions.at(index) <= end)
            .filter_map(move |index| {
                let position = self.positions.at(index);
                if position >= start {
                    self.values
                        .at(index)
                        .as_ref()
                        .map(|values| (position, values))
                } else {
                    None
                }
            })
    }
    fn insert_space(&mut self, position: usize, length: usize) {
        let partition = self.positions.partition(position);
        if self.positions.at(partition) == position {
            let occupied = self.values.at(partition).is_some();
            if partition == 0 {
                if occupied {
                    self.positions.insert(1, 0);
                    self.values.insert(0, None);
                }
                self.positions.shift(partition, length as isize);
            } else {
                self.positions.shift(
                    if occupied { partition - 1 } else { partition },
                    length as isize,
                );
            }
        } else {
            self.positions.shift(partition, length as isize);
        }
    }
    fn delete(&mut self, position: usize, length: usize) {
        if length == 0 {
            return;
        }
        if position == 0 {
            while self.positions.count() > 1 && self.positions.at(1) <= length {
                self.positions.remove(1);
                self.values.remove(0);
            }
            self.positions.shift(0, -(length as isize));
            if self.positions.length() == 0 {
                self.values.set(0, None);
            }
        } else {
            let partition = self.positions.partition(position);
            let at_start = position == self.positions.at(partition);
            let index = partition + usize::from(!at_start);
            while self.positions.at(index) < position + length {
                self.positions.remove(index);
                self.values.remove(index);
            }
            self.positions
                .shift(partition - usize::from(at_start), -(length as isize));
        }
    }
}

#[derive(Clone)]
enum Span {
    Insertion(Run),
    Deletion {
        position: usize,
        value: EditionCount,
    },
}

#[derive(Clone)]
struct Log {
    insertions: Runs,
    deletions: Deletions,
    steps: Vec<Vec<Span>>,
}

impl Log {
    fn new(length: usize) -> Self {
        Self {
            insertions: Runs::new(length),
            deletions: Deletions::new(length),
            steps: Vec::new(),
        }
    }

    fn insert_space(&mut self, position: usize, length: usize) {
        self.insertions.insert_space(position, length);
        self.deletions.insert_space(position, length);
    }

    fn delete(&mut self, position: usize, length: usize) {
        let previous = self.deletions.get(&position).cloned();
        self.insertions.delete(position..position + length);
        self.deletions.delete(position, length);
        if let Some(values) = previous {
            self.deletions.insert(position, values);
        }
    }

    fn push_deletion(&mut self, position: usize, value: EditionCount) {
        if self.deletions.get(&position).is_none() {
            self.deletions.insert(position, Vec::new());
        }
        let values = self
            .deletions
            .get_mut(&position)
            .expect("source deletion position");
        if let Some(last) = values.last_mut()
            && last.edition == value.edition
        {
            last.count += value.count;
        } else {
            values.push(value);
        }
    }

    fn pop_deletion(&mut self, position: usize) {
        let values = self
            .deletions
            .get_mut(&position)
            .expect("source deletion edition");
        let last = values.last_mut().expect("source deletion count");
        if last.count == 1 {
            values.pop();
        } else {
            last.count -= 1;
        }
        if values.is_empty() {
            self.deletions.remove(&position);
        }
    }

    fn save_range(&mut self, position: usize, length: usize) {
        let end = position + length;
        let mut spans = Vec::new();
        let mut at = position;
        if self.insertions.at(at) == 0 {
            at = self.insertions.end(at);
        }
        while at < end {
            let next = self.insertions.end(at);
            spans.push(Span::Insertion(Run {
                start: at,
                end: next.min(end),
                edition: self.insertions.at(at),
            }));
            at = self.insertions.end(next);
        }
        for (position, values) in self.deletions.range(position + 1..=end) {
            spans.extend(
                values
                    .iter()
                    .copied()
                    .map(|value| Span::Deletion { position, value }),
            );
        }
        self.steps.push(spans);
    }

    fn save_history_for_delete(&mut self, position: usize, length: usize) {
        self.save_range(position, length);
        let values: Vec<_> = self
            .deletions
            .range(position + 1..=position + length)
            .map(|(at, values)| (at, values.clone()))
            .collect();
        for (at, values) in values {
            for value in values {
                self.push_deletion(position, value);
            }
            self.deletions.remove(&at);
        }
    }

    fn delete_saving_history(&mut self, position: usize, length: usize) {
        self.save_history_for_delete(position, length);
        self.delete(position, length);
    }

    fn restore_deletion(&mut self, position: usize, length: usize) {
        let values = self
            .deletions
            .remove(&(position + length))
            .expect("source deleted text history");
        self.deletions.insert(position, values);
        self.pop_deletion(position);
        for span in self
            .steps
            .pop()
            .expect("source deletion step")
            .into_iter()
            .rev()
        {
            match span {
                Span::Insertion(run) => self.insertions.fill(run.start..run.end, run.edition),
                Span::Deletion {
                    position: at,
                    value,
                } => {
                    for _ in 0..value.count {
                        self.pop_deletion(position);
                    }
                    if self.deletions.get(&at).is_none() {
                        self.deletions.insert(at, Vec::new());
                    }
                    self.deletions
                        .get_mut(&at)
                        .expect("source front deletion")
                        .insert(0, value);
                }
            }
        }
    }

    fn saved(&mut self) {
        let mut start = 0;
        while start < self.insertions.length() {
            let end = self.insertions.end(start);
            if self.insertions.at(start) == 3 {
                self.insertions.fill(start..end, 2);
            }
            start = end;
        }
        for index in 0..self.deletions.values.len() {
            if let Some(values) = self.deletions.values.at_mut(index) {
                for value in values {
                    if value.edition == 3 {
                        value.edition = 2;
                    }
                }
            }
        }
        for step in &mut self.steps {
            for span in step {
                let edition = match span {
                    Span::Insertion(run) => &mut run.edition,
                    Span::Deletion { value, .. } => &mut value.edition,
                };
                if *edition == 3 {
                    *edition = 2;
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct History {
    log: Log,
    reversions: Option<Log>,
    action: usize,
    save: Option<usize>,
    detach: Option<usize>,
}

#[derive(Clone, Copy)]
pub(crate) enum Event {
    Insert(usize, usize),
    Delete(usize, usize),
    UndoInsert(usize, usize),
    UndoDelete(usize, usize),
    RedoInsert(usize, usize),
    RedoDelete(usize, usize),
    Saved,
    ClearUndo,
    ClearChanges(usize),
}

impl Event {
    pub(crate) fn apply(self, history: &mut History) {
        match self {
            Self::Insert(at, length) => history.insert_new(at, length),
            Self::Delete(at, length) => history.delete_new(at, length),
            Self::UndoInsert(at, length) => history.undo_insert(at, length),
            Self::UndoDelete(at, length) => history.undo_delete(at, length),
            Self::RedoInsert(at, length) => history.redo_insert(at, length),
            Self::RedoDelete(at, length) => history.redo_delete(at, length),
            Self::Saved => history.saved(),
            Self::ClearUndo => history.clear_undo(),
            Self::ClearChanges(length) => *history = History::new(length),
        }
    }

    pub(crate) fn replay_after_captured_save(self, history: &mut History) {
        if !matches!(self, Self::Saved) {
            self.apply(history);
        }
    }
}

impl History {
    pub(crate) fn new(length: usize) -> Self {
        Self {
            log: Log::new(length),
            reversions: None,
            action: 0,
            save: Some(0),
            detach: None,
        }
    }

    fn append(&mut self) {
        if self.save.is_some_and(|save| self.action < save) {
            self.save = None;
            self.detach.get_or_insert(self.action);
        } else if self.detach.is_some_and(|detach| detach > self.action) {
            self.detach = Some(self.action);
        }
        self.action += 1;
    }

    fn insert(&mut self, position: usize, length: usize, before_save: bool) {
        self.log.insert_space(position, length);
        self.log
            .insertions
            .fill(position..position + length, if before_save { 2 } else { 3 });
        if let Some(reversions) = &mut self.reversions {
            reversions.insert_space(position, length);
            if before_save {
                reversions.restore_deletion(position, length);
            }
        }
    }

    fn delete(&mut self, position: usize, length: usize, before_save: bool, detached: bool) {
        self.log.delete_saving_history(position, length);
        self.log.push_deletion(
            position,
            EditionCount {
                edition: if before_save { 2 } else { 3 },
                count: 1,
            },
        );
        if let Some(reversions) = &mut self.reversions {
            if detached {
                reversions.save_history_for_delete(position, length);
            }
            reversions.delete(position, length);
        }
    }

    pub(crate) fn insert_new(&mut self, position: usize, length: usize) {
        self.append();
        self.insert(position, length, false);
    }

    pub(crate) fn delete_new(&mut self, position: usize, length: usize) {
        self.append();
        self.delete(
            position,
            length,
            false,
            self.detach.is_some_and(|detach| detach <= self.action),
        );
    }

    fn before_undo(&mut self) -> (bool, bool) {
        let before_save = self.save.is_none_or(|save| save >= self.action);
        if before_save && self.reversions.is_none() {
            self.reversions = Some(Log::new(self.log.insertions.length()));
        }
        (
            before_save,
            self.detach.is_some_and(|detach| detach < self.action),
        )
    }

    pub(crate) fn undo_insert(&mut self, position: usize, length: usize) {
        let (before_save, detached) = self.before_undo();
        self.log.delete(position, length);
        if let Some(reversions) = &mut self.reversions {
            reversions.delete_saving_history(position, length);
            if before_save && !detached {
                reversions.push_deletion(
                    position,
                    EditionCount {
                        edition: 1,
                        count: 1,
                    },
                );
            }
        }
        self.action -= 1;
    }

    pub(crate) fn undo_delete(&mut self, position: usize, length: usize) {
        let (_, detached) = self.before_undo();
        self.log.insert_space(position, length);
        self.log.restore_deletion(position, length);
        if let Some(reversions) = &mut self.reversions {
            reversions.insert_space(position, length);
            if !detached {
                reversions.insertions.fill(position..position + length, 1);
            }
        }
        self.action -= 1;
    }

    fn after_redo(&mut self) {
        if self.save.is_some_and(|save| save <= self.action) {
            self.reversions = None;
        }
        self.action += 1;
    }

    pub(crate) fn redo_insert(&mut self, position: usize, length: usize) {
        let before_save = self.save.is_none_or(|save| save > self.action)
            && !self.detach.is_some_and(|detach| detach <= self.action);
        self.insert(position, length, before_save);
        self.after_redo();
    }

    pub(crate) fn redo_delete(&mut self, position: usize, length: usize) {
        let before_save = self.save.is_some_and(|save| save > 0 && save > self.action);
        self.delete(
            position,
            length,
            before_save,
            self.detach.is_some_and(|detach| detach <= self.action),
        );
        self.after_redo();
    }

    pub(crate) fn saved(&mut self) {
        self.log.saved();
        self.reversions = None;
        self.save = Some(self.action);
        self.detach = None;
    }

    pub(crate) fn clear_undo(&mut self) {
        self.action = 0;
        self.save = Some(0);
    }

    pub(crate) fn line_bits(&self, start: usize, next: usize, end: usize) -> u8 {
        let mut result = 0;
        let mut at = start;
        while at < next {
            let edition = self.log.insertions.at(at);
            let reverted = self
                .reversions
                .as_ref()
                .is_some_and(|log| log.insertions.at(at) != 0);
            let edition = if reverted {
                if edition == 0 { 1 } else { 4 }
            } else {
                edition
            };
            if edition != 0 {
                result |= 1 << (edition - 1);
            }
            let base_end = self.log.insertions.end(at);
            at = self
                .reversions
                .as_ref()
                .map_or(base_end, |log| base_end.min(log.insertions.end(at)));
        }
        let mut positions: BTreeSet<_> = self
            .log
            .deletions
            .range(start..=end)
            .map(|(at, _)| at)
            .collect();
        if let Some(log) = &self.reversions {
            positions.extend(log.deletions.range(start..=end).map(|(at, _)| at));
        }
        for at in positions {
            let mut bits = self.log.deletions.get(&at).map_or(0, |values| {
                values
                    .iter()
                    .fold(0u8, |bits, value| bits | (1 << (value.edition - 1)))
            });
            if self
                .reversions
                .as_ref()
                .is_some_and(|log| log.deletions.contains_key(&at))
            {
                bits |= if bits & 6 == 0 { 1 } else { 8 };
            }
            result |= bits;
        }
        result
    }
}
