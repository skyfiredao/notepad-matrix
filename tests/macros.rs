use notepad_matrix::config::ConfigStore;
use notepad_matrix::input::EditorSession;
use notepad_matrix::macros::{MacroLibrary, MacroSaveTask, MacroStep, RepeatMode, RepeatTracker};

#[test]
fn macro_save_queue_waits_for_previous_publication_not_only_its_disk_commit()
-> Result<(), Box<dyn std::error::Error>> {
    use std::cell::{Cell, RefCell};
    use std::future::{Future, poll_fn};
    use std::rc::Rc;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    struct WakeThread(std::thread::Thread);
    impl Wake for WakeThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let a = MacroLibrary::save_current(&store, "A", &[MacroStep::Tab])?;
    let published = Rc::new(Cell::new(false));
    let publication_waker = Rc::new(RefCell::new(None::<Waker>));
    let previous_publication = poll_fn({
        let published = published.clone();
        let publication_waker = publication_waker.clone();
        move |cx| {
            if published.get() {
                Poll::Ready(())
            } else {
                *publication_waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    });
    let mut b = Box::pin(MacroSaveTask::after_publication(
        previous_publication,
        home.path().to_path_buf(),
        "B".into(),
        vec![MacroStep::Newline],
    ));
    let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(b.as_mut().poll(&mut context), Poll::Pending));
    assert!(
        publication_waker.borrow().is_some(),
        "B must await A's publication, not merely be pending on a racing disk worker"
    );
    assert_eq!(
        MacroLibrary::load(&store)?
            .macros
            .iter()
            .map(|saved| saved.name.as_str())
            .collect::<Vec<_>>(),
        vec!["A"],
        "B must not commit or publish while A's completed write awaits publication"
    );
    let mut visible = a;
    assert_eq!(visible.macros.len(), 1);
    published.set(true);
    publication_waker
        .borrow_mut()
        .take()
        .ok_or("missing publication waker")?
        .wake();
    visible = loop {
        match b.as_mut().poll(&mut context) {
            Poll::Ready(result) => break result?,
            Poll::Pending => std::thread::park(),
        }
    };
    assert_eq!(
        visible
            .macros
            .iter()
            .map(|saved| saved.name.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B"]
    );
    assert_eq!(visible, MacroLibrary::load(&store)?);
    Ok(())
}

#[test]
fn macro_save_worker_stays_pending_without_blocking_while_config_is_locked()
-> Result<(), Box<dyn std::error::Error>> {
    use std::future::Future;
    use std::sync::{Arc, mpsc};
    use std::task::{Context, Poll, Wake, Waker};

    struct WakeThread(std::thread::Thread);
    impl Wake for WakeThread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let home = tempfile::tempdir()?;
    let lock_home = home.path().to_path_buf();
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let locker = std::thread::spawn(move || {
        ConfigStore::under_home(&lock_home).update_json(
            std::path::Path::new("macros.json"),
            &serde_json::json!({"schema_version":1,"macros":[]}),
            |_| {
                locked_tx.send(()).map_err(std::io::Error::other)?;
                release_rx.recv().map_err(std::io::Error::other)?;
                Ok(())
            },
        )
    });
    locked_rx.recv()?;
    let mut task = Box::pin(MacroSaveTask::start(
        home.path().to_path_buf(),
        "captured name".into(),
        vec![MacroStep::ReplaceSelection("captured text".into())],
    )?);
    let waker = Waker::from(Arc::new(WakeThread(std::thread::current())));
    let mut context = Context::from_waker(&waker);
    assert!(matches!(task.as_mut().poll(&mut context), Poll::Pending));
    release_tx.send(())?;
    locker
        .join()
        .map_err(|_| "configuration locker panicked")??;
    let library = loop {
        match task.as_mut().poll(&mut context) {
            Poll::Ready(result) => break result?,
            Poll::Pending => std::thread::park(),
        }
    };
    assert_eq!(library.macros.len(), 1);
    assert_eq!(library.macros[0].name, "captured name");
    assert_eq!(
        library.macros[0].steps,
        vec![MacroStep::ReplaceSelection("captured text".into())]
    );
    Ok(())
}

#[test]
fn macro_count_input_matches_source_minimum_and_four_character_control() {
    assert_eq!(
        RepeatMode::from_count_input("0").expect("source minimum"),
        RepeatMode::Count(1)
    );
    assert_eq!(
        RepeatMode::from_count_input("").expect("source empty integer"),
        RepeatMode::Count(1)
    );
    assert_eq!(
        RepeatMode::from_count_input("9999").expect("four digits"),
        RepeatMode::Count(9999)
    );
    assert!(RepeatMode::from_count_input("10000").is_err());
}

#[test]
fn repeat_count_finishes_exactly_and_eof_stops_no_movement_and_tracks_line_deletion()
-> Result<(), Box<dyn std::error::Error>> {
    let mut count = RepeatTracker::new(RepeatMode::Count(3), 1, 0)?;
    assert!(count.complete_iteration(1, 0));
    assert!(count.complete_iteration(1, 0));
    assert!(!count.complete_iteration(1, 0));
    assert_eq!(count.completed(), 3);
    let mut stationary = RepeatTracker::new(RepeatMode::UntilEnd, 10, 4)?;
    assert!(
        !stationary.complete_iteration(10, 4),
        "source stops after one iteration without line movement/deletion"
    );
    let mut deleting = RepeatTracker::new(RepeatMode::UntilEnd, 3, 0)?;
    assert!(deleting.complete_iteration(2, 0));
    assert!(deleting.complete_iteration(1, 0));
    assert!(!deleting.complete_iteration(1, 0));
    assert_eq!(deleting.completed(), 3);
    let mut extending = RepeatTracker::new(RepeatMode::UntilEnd, 3, 0)?;
    assert!(extending.complete_iteration(4, 1));
    assert!(extending.complete_iteration(5, 2));
    assert!(
        !extending.complete_iteration(6, 3),
        "growing lines must not move the original EOF goal forever"
    );
    let mut cancelled = RepeatTracker::new(RepeatMode::Count(9999), 1, 0)?;
    cancelled.cancel();
    assert!(cancelled.is_finished());
    assert!(!cancelled.complete_iteration(1, 0));
    assert_eq!(cancelled.completed(), 0);
    assert!(RepeatTracker::new(RepeatMode::Count(0), 1, 0).is_err());
    assert!(RepeatTracker::new(RepeatMode::Count(10000), 1, 0).is_err());
    let mut reversing = RepeatTracker::new(RepeatMode::UntilEnd, 3, 1)?;
    assert!(reversing.complete_iteration(3, 2));
    assert!(reversing.complete_iteration(3, 1));
    assert!(!reversing.complete_iteration(3, 2));
    assert_eq!(
        reversing.completed(),
        3,
        "source stops a reversing line walk"
    );
    Ok(())
}

#[test]
fn named_macro_persists_typed_steps_without_replacing_duplicate_names()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    let steps = vec![
        MacroStep::ReplaceSelection("中😀".into()),
        MacroStep::Newline,
        MacroStep::LineEdge {
            end: false,
            extend: true,
        },
    ];
    MacroLibrary::save_current(&store, "Insert Unicode", &steps)?;
    MacroLibrary::save_current(&store, "Insert Unicode", &[MacroStep::Tab])?;
    let library = MacroLibrary::load(&store)?;
    assert_eq!(
        library.macros.len(),
        2,
        "addCurrentMacro appends, it does not overwrite by name"
    );
    assert_eq!(library.macros[0].name, "Insert Unicode");
    assert_eq!(library.macros[0].steps, steps);
    assert_eq!(library.macros[1].steps, vec![MacroStep::Tab]);
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.root().join("macros.json"))?)?;
    assert_eq!(json["schema_version"], 1);
    assert_eq!(
        json["macros"][0]["steps"][0],
        serde_json::json!({"type":"text", "text":"中😀"})
    );
    Ok(())
}

#[test]
fn macro_json_rejects_unknown_commands_and_failed_save_preserves_the_original()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    MacroLibrary::save_current(&store, "valid", &[MacroStep::Tab])?;
    let path = store.root().join("macros.json");
    let original = std::fs::read(&path)?;
    assert!(
        MacroLibrary::save_current(
            &store,
            "unsupported",
            &[
                MacroStep::ReplaceSelection("must not partially save".into()),
                MacroStep::Unsupported("Run command")
            ]
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path)?, original);
    for bad in [
        serde_json::json!({"schema_version":1,"macros":[{"name":"bad","steps":[{"type":"text_transform","source_id":41001}]}]}),
        serde_json::json!({"schema_version":1,"macros":[{"name":"bad","steps":[{"type":"command","id":41001}]}]}),
        serde_json::json!({"schema_version":1,"macros":[{"name":"bad","steps":[{"type":"delete","right":"true"}]}]}),
        serde_json::json!({"schema_version":2,"macros":[]}),
    ] {
        let bytes = serde_json::to_vec(&bad)?;
        std::fs::write(&path, &bytes)?;
        assert!(MacroLibrary::load(&store).is_err());
        assert!(MacroLibrary::save_current(&store, "new", &[MacroStep::Tab]).is_err());
        assert_eq!(std::fs::read(&path)?, bytes);
    }
    Ok(())
}

#[test]
fn macro_save_preserves_unknown_metadata_and_all_existing_typed_step_variants()
-> Result<(), Box<dyn std::error::Error>> {
    use notepad_matrix::menu::BookmarkCommand;
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    store.update_json(
        std::path::Path::new("macros.json"),
        &serde_json::json!({
            "schema_version":1, "future":{"keep":true},
            "macros":[{"name":"existing", "future":17,"steps":[{"type":"tab","future":23}]}]
        }),
        |_| Ok(()),
    )?;
    let steps = vec![
        MacroStep::ReplaceSelection("a\0中".into()),
        MacroStep::CharacterText("\0€".into()),
        MacroStep::Newline,
        MacroStep::Tab,
        MacroStep::BackTab,
        MacroStep::Rectangle(notepad_matrix::macros::RectangleMovement::Down),
        MacroStep::Character {
            right: true,
            extend: false,
        },
        MacroStep::Vertical {
            down: false,
            extend: true,
        },
        MacroStep::LineEdge {
            end: true,
            extend: false,
        },
        MacroStep::DocumentEdge {
            end: false,
            extend: true,
        },
        MacroStep::Word {
            forward: true,
            extend: false,
            part: true,
        },
        MacroStep::DisplayEdge { end: false },
        MacroStep::Page {
            down: true,
            extend: true,
            rectangle: true,
        },
        MacroStep::DeleteWord {
            forward: false,
            to_line_edge: true,
        },
        MacroStep::BackspaceNotLine,
        MacroStep::TextTransform(42016),
        MacroStep::MultiSelection(notepad_matrix::menu::MultiSelectionCommand::Next(
            notepad_matrix::editor::OccurrenceOptions {
                match_case: true,
                whole_word: false,
            },
        )),
        MacroStep::Delete { right: false },
        MacroStep::ClearSelection,
        MacroStep::DeleteLine,
        MacroStep::Duplicate { line: true },
        MacroStep::SelectAll,
        MacroStep::Copy { cut: true },
        MacroStep::Paste,
        MacroStep::CopyPath(notepad_matrix::clipboard::CopyPath::AllPaths),
        MacroStep::GoToLine(42),
        MacroStep::Bookmark(BookmarkCommand::Toggle),
        MacroStep::Bookmark(BookmarkCommand::Next),
        MacroStep::Bookmark(BookmarkCommand::Previous),
        MacroStep::Bookmark(BookmarkCommand::ClearAll),
    ];
    let library = MacroLibrary::save_current(&store, "all typed edits", &steps)?;
    assert_eq!(library.macros[1].steps, steps);
    let document = store.load_json(std::path::Path::new("macros.json"))?;
    assert_eq!(document["future"]["keep"], true);
    assert_eq!(document["macros"][0]["future"], 17);
    assert_eq!(document["macros"][0]["steps"][0]["future"], 23);
    Ok(())
}

#[test]
fn persisted_typed_macro_repeat_edits_and_single_undo_match_count_and_eof()
-> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let store = ConfigStore::under_home(home.path());
    MacroLibrary::save_current(
        &store,
        "insert A",
        &[MacroStep::ReplaceSelection("A".into())],
    )?;
    MacroLibrary::save_current(&store, "delete lines", &[MacroStep::DeleteLine])?;
    let library = MacroLibrary::load(&store)?;
    for (saved, mode, initial, expected) in [
        (&library.macros[0], RepeatMode::Count(3), "base", "AAAbase"),
        (
            &library.macros[1],
            RepeatMode::UntilEnd,
            "one\ntwo\nthree",
            "",
        ),
    ] {
        let mut editor = EditorSession::from_file_text(initial);
        let mut tracker = RepeatTracker::new(mode, editor.document().line_count(), 0)?;
        editor.begin_undo_action();
        loop {
            for step in &saved.steps {
                match step {
                    MacroStep::ReplaceSelection(text) => editor.insert_text(text)?,
                    MacroStep::DeleteLine => editor.delete_current_line()?,
                    _ => {
                        return Err(
                            "fixture only uses text insertion and current-line deletion".into()
                        );
                    }
                }
            }
            if !tracker.complete_iteration(editor.document().line_count(), 0) {
                break;
            }
        }
        editor.end_undo_action();
        assert_eq!(tracker.completed(), 3);
        assert_eq!(editor.committed_text(), expected);
        assert!(editor.undo());
        assert_eq!(editor.committed_text(), initial);
        assert!(!editor.undo(), "one Undo covers every repetition");
    }
    Ok(())
}

#[test]
fn macro_undo_group_keeps_one_history_boundary_and_replays_line_markers()
-> Result<(), Box<dyn std::error::Error>> {
    let mut editor = EditorSession::from_file_text("base\nlast");
    editor.toggle_bookmark(Some(1))?;
    editor.begin_undo_action();
    editor.insert_text("A\n")?;
    editor.insert_text("B\n")?;
    editor.end_undo_action();
    assert_eq!(editor.committed_text(), "A\nB\nbase\nlast");
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![3]
    );
    assert!(editor.undo());
    assert_eq!(editor.committed_text(), "base\nlast");
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(!editor.is_modified());
    assert!(!editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.committed_text(), "A\nB\nbase\nlast");
    assert_eq!(
        editor.document().bookmarked_lines().collect::<Vec<_>>(),
        vec![3]
    );
    Ok(())
}
