use super::*;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Minimize,
    Maximize,
    Restore,
    Close,
}

pub(super) fn render(icon: Icon, id: &'static str, dark: bool) -> AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .size(px(10.))
        .overflow_hidden()
        .flex_shrink_0()
        .relative()
        .top(px(1.))
        .child(
            gpui::canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let origin = gpui::point(
                        px(f32::from(bounds.origin.x).round()),
                        px(f32::from(bounds.origin.y).round()),
                    );
                    let colour = rgb(if dark { 0xcccccc } else { 0x999999 });
                    let rectangle = |x, y, extent| {
                        Bounds::new(
                            origin + gpui::point(px(x), px(y)),
                            size(px(extent), px(extent)),
                        )
                    };
                    match icon {
                        Icon::Minimize => window.paint_quad(gpui::fill(
                            Bounds::new(
                                origin + gpui::point(px(0.), px(5.)),
                                size(px(10.), px(1.)),
                            ),
                            colour,
                        )),
                        Icon::Maximize => window.paint_quad(gpui::outline(
                            rectangle(0., 0., 10.),
                            colour,
                            gpui::BorderStyle::Solid,
                        )),
                        Icon::Restore => {
                            for (x, y, width, height) in [
                                (2., 0., 8., 1.),
                                (9., 0., 1., 8.),
                                (2., 0., 1., 2.),
                                (8., 7., 2., 1.),
                            ] {
                                window.paint_quad(gpui::fill(
                                    Bounds::new(
                                        origin + gpui::point(px(x), px(y)),
                                        size(px(width), px(height)),
                                    ),
                                    colour,
                                ));
                            }
                            window.paint_quad(gpui::outline(
                                rectangle(0., 2., 8.),
                                colour,
                                gpui::BorderStyle::Solid,
                            ));
                        }
                        Icon::Close => {
                            let mut path = gpui::PathBuilder::stroke(px(1.));
                            path.move_to(origin + gpui::point(px(0.), px(0.)));
                            path.line_to(origin + gpui::point(px(10.), px(10.)));
                            path.move_to(origin + gpui::point(px(10.), px(0.)));
                            path.line_to(origin + gpui::point(px(0.), px(10.)));
                            match path.build() {
                                Ok(path) => window.paint_path(path, colour),
                                Err(error) => eprintln!("Could not paint close icon: {error}"),
                            }
                        }
                    }
                },
            )
            .size_full(),
        )
        .into_any_element()
}
