use super::*;

pub fn capture(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let platform = gpui_platform::current_platform(true);
    let mut app = gpui::HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let mut workspace = crate::workspace::Workspace::new();
    let empty = workspace.active().id();
    let first = workspace.append_recovered_buffer_with_size(
        EditorSession::new(""),
        None,
        "新文件 2".into(),
        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
        0,
    )?;
    workspace.close_buffer(empty, crate::workspace::CloseDisposition::IfUnmodified)?;
    workspace.set_pinned(first, true)?;
    let selected = workspace.append_recovered_buffer_with_size(
        EditorSession::new(""),
        None,
        "新文件 1".into(),
        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
        0,
    )?;
    workspace.set_pinned(selected, true)?;
    workspace.append_recovered_buffer_with_size(
        EditorSession::new(""),
        None,
        "浏览器标签.txt".into(),
        crate::files::UnicodeFormat::Utf8(crate::files::Utf8Format::Plain),
        0,
    )?;
    workspace.activate(selected)?;
    let text = concat!(
        "getmeili/meilisearch   latest   31fbc3072667   8 months ago   145MB\n",
        "getmeili/docs-scraper   latest   f41383e7e97   14 months ago   844MB\n",
        "\n",
        "源代码库的数据迁移和 git lfs 的设计缺陷\n",
        "https://docs.github.com/en/repositories/creating-and-managing-repositories/repository-limits\n",
        "https://docs.github.com/en/repositories/working-with-files/managing-large-files/about-git-large-file-storage\n",
        "https://docs.github.com/en/actions/learn-github-actions/usage-limits-billing-and-administration\n",
        "https://docs.github.com/en/get-started/using-git/troubleshooting-the-2-gb-push-limit\n",
        "https://github.com/dead-claudia/github-limits\n",
        "\n",
        "gerrit 后台 hooks 的 GitHub 替代方案\n",
        "- gerrit vote 的 GitHub 替代方案\n",
        "如何使用 GitHub token\n",
        "gerrit topic 原子提交功能的 GitHub 替代方案\n",
        "批量 GitHub 代码库的设置方法\n",
        "如何实现 gerrit events 的高效监听\n",
        "按需驱动任务\n",
        "高级审核批复\n",
        "如何实现数据的双向对齐\n",
        "\n\n\n\n\n\n\n\n\n"
    );
    workspace.active_mut().editor.insert_text(text)?;
    workspace
        .active_mut()
        .editor
        .convert_line_endings(crate::input::LineEnding::Lf)?;
    let caret = ByteOffset(text.lines().take(2).map(|line| line.len() + 1).sum());
    workspace.active_mut().editor.select(Selection {
        anchor: caret,
        head: caret,
    })?;
    let window = app.open_window(size(px(1080.), px(668.)), |window, cx| {
        cx.new(|cx| {
            let mut view = MatrixWindow::new(false, window, cx);
            view.workspace = workspace;
            view.reset_active_view(window, cx);
            view
        })
    })?;
    for _ in 0..3 {
        app.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))?;
        app.run_until_parked();
    }
    app.capture_screenshot(window.into())?.save(path)?;
    Ok(())
}

pub fn capture_search(
    path: &std::path::Path,
    page: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !matches!(page, "find" | "replace" | "files" | "results") {
        return Err("expected find, replace, files or results".into());
    }
    let platform = gpui_platform::current_platform(true);
    let mut app = gpui::HeadlessAppContext::with_platform(
        platform.text_system(),
        std::sync::Arc::new(()),
        gpui_platform::current_headless_renderer,
    );
    let main = app.open_window(size(px(1080.), px(668.)), |window, cx| {
        cx.new(|cx| MatrixWindow::new(false, window, cx))
    })?;
    app.update(|cx| {
        main.update(cx, |view, window, cx| {
            match page {
                "replace" => view.show_replace(window, cx),
                "files" => view.show_find_in_files(window, cx),
                _ => {
                    if page == "results" {
                        *view.editor_mut() = EditorSession::new(
                            "KeyNotFoundException KeyNotFoundException\nKeyNotFoundException",
                        );
                    }
                    view.show_find(window, cx);
                }
            }
            view.find_query.update(cx, |field, cx| {
                field.set_text("KeyNotFoundException", window, cx)
            });
        })
    })?;
    for _ in 0..3 {
        app.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))?;
        app.run_until_parked();
    }
    if page == "results" {
        app.update(|cx| {
            main.update(cx, |view, window, cx| {
                view.find_all_documents(false, window, cx)
            })
        })?;
        for _ in 0..3 {
            app.update_window(main.into(), |_, window, cx| window.draw(cx).clear(cx))?;
            app.run_until_parked();
        }
        app.capture_screenshot(main.into())?.save(path)?;
        return Ok(());
    }
    let dialog = app
        .update(|cx| {
            main.update(cx, |view, _, _| {
                view.floating_windows
                    .get(&floating_windows::DialogKind::Find)
                    .copied()
            })
        })?
        .ok_or("search dialog did not open")?;
    for _ in 0..3 {
        app.update_window(dialog.into(), |_, window, cx| window.draw(cx).clear(cx))?;
        app.run_until_parked();
    }
    app.capture_screenshot(dialog.into())?.save(path)?;
    Ok(())
}
