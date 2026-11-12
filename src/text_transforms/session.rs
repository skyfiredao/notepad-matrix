use super::{
    TransformCommand, TransformOptions, TransformSelection, TransformSelectionMode, plan_transform,
};
use crate::editor::{
    ByteOffset, EditSelectionPolicy, Selection, SelectionRange, SelectionSet, TextTransform,
};
use crate::input::EditorSession;

pub fn comment_syntax(
    language: crate::syntax::Language,
    definition: Option<&crate::udl::Definition>,
) -> Result<super::CommentSyntax, String> {
    use super::CommentSyntax;
    use crate::syntax::Language;
    let nonempty = |value: &str| (!value.is_empty()).then(|| value.to_owned());
    let pair = |start: &str, end: &str| nonempty(start).zip(nonempty(end));
    match language {
        Language::NormalText | Language::Builtin(0) => Ok(CommentSyntax::default()),
        Language::UserDefined(_) => {
            let definition = definition
                .ok_or_else(|| "The selected User Defined language is unavailable".to_owned())?;
            Ok(CommentSyntax {
                line: nonempty(&definition.comments.line_open),
                stream: pair(
                    &definition.comments.block_open,
                    &definition.comments.block_close,
                ),
                at_line_start: definition.line_comment_placement
                    == crate::udl::LineCommentPlacement::BeginningOfLine,
                ..CommentSyntax::default()
            })
        }
        Language::Json | Language::Builtin(_) => {
            let id = match language {
                Language::Builtin(id) => id,
                _ => 57,
            };
            let profile = crate::languages::catalog()
                .map_err(|error| error.to_string())?
                .by_id(id)
                .ok_or_else(|| "The selected language is unavailable".to_owned())?;
            Ok(CommentSyntax {
                line: nonempty(&profile.comment_line),
                stream: pair(&profile.comment_start, &profile.comment_end),
                ..CommentSyntax::default()
            })
        }
    }
}

pub fn apply_command(
    session: &mut EditorSession,
    id: u32,
    mut options: TransformOptions,
) -> Result<(), String> {
    let command =
        TransformCommand::from_source_id(id).ok_or_else(|| format!("Unknown text command {id}"))?;
    if session.has_composition() {
        return Err("Finish text composition before transforming text".into());
    }
    let original = session.display_selections();
    options.main_selection = original.primary_index();
    options.read_only = session.is_read_only();
    options.line_ending = session.line_ending().as_str().into();
    if original.is_rectangular() {
        let main = original.primary();
        let anchor = session
            .document()
            .display_column(main.selection.anchor, options.tab_width)
            .map_err(|error| error.to_string())?
            + main.anchor_virtual;
        let caret = session
            .document()
            .display_column(main.selection.head, options.tab_width)
            .map_err(|error| error.to_string())?
            + main.head_virtual;
        options.selection_mode = TransformSelectionMode::Rectangle {
            from_column: anchor.min(caret),
            to_column: anchor.max(caret),
        };
    }
    let selections: Vec<_> = original
        .ranges()
        .iter()
        .map(|range| TransformSelection {
            anchor: range.selection.anchor.0,
            caret: range.selection.head.0,
            anchor_virtual_space: range.anchor_virtual,
            caret_virtual_space: range.head_virtual,
        })
        .collect();
    if command == TransformCommand::SplitLines {
        use ropey::LineType;
        let snapshot = session.display_snapshot();
        let range = original.primary().selection.range();
        let first = snapshot.byte_to_line_idx(range.start.0, LineType::LF_CR);
        let mut last = snapshot.byte_to_line_idx(range.end.0, LineType::LF_CR);
        if last > first && range.end.0 == snapshot.line_to_byte_idx(last, LineType::LF_CR) {
            last -= 1;
        }
        options.split_breaks.retain(|&position| {
            if position > snapshot.len() {
                return true;
            }
            let row = snapshot.byte_to_line_idx(position, LineType::LF_CR);
            row >= first && row <= last
        });
    }
    let plan = plan_transform(&session.display_text(), command, &selections, &options)
        .map_err(|error| error.to_string())?;
    let primary = original
        .primary_index()
        .min(plan.selections.len().saturating_sub(1));
    let ranges = plan
        .selections
        .into_iter()
        .map(|range| SelectionRange {
            selection: Selection {
                anchor: ByteOffset(range.anchor),
                head: ByteOffset(range.caret),
            },
            anchor_virtual: range.anchor_virtual_space,
            head_virtual: range.caret_virtual_space,
        })
        .collect();
    let selections = if original.is_rectangular() {
        SelectionSet::rectangle(ranges, primary)
    } else {
        SelectionSet::new(ranges, primary)
    }
    .map_err(|error| error.to_string())?;
    let edits: Vec<_> = plan
        .replacements
        .into_iter()
        .map(|edit| TextTransform {
            range: ByteOffset(edit.range.start)..ByteOffset(edit.range.end),
            text: edit.text,
        })
        .collect();
    session
        .apply_multi(&edits, EditSelectionPolicy::Explicit(selections))
        .map_err(|error| error.to_string())
}
