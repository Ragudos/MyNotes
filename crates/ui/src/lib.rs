use fltk::prelude::*;

struct State {
    doc: std::rc::Rc<std::cell::RefCell<editor_state::document::Document>>,
    cursor_visible: bool,
    scroll_offset: usize,
}

pub struct TextEditor {}

impl TextEditor {
    pub fn new_editor(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        doc: std::rc::Rc<std::cell::RefCell<editor_state::document::Document>>,
    ) -> fltk::group::Group {
        // 1. Open the Group.
        // Any widgets created after this will automatically be added as children of 'grp'.
        let grp = fltk::group::Group::default().with_pos(x, y).with_size(w, h);

        let state = std::rc::Rc::new(std::cell::RefCell::new(State {
            doc,
            cursor_visible: false,
            scroll_offset: 0,
        }));

        let line_height = 20;
        let font_size = 14;
        let left_padding = 6; // Keeping your variables

        // 2. Create the Canvas (your custom drawing widget).
        // Notice we subtract 15 from the width to make room for the scrollbar.
        let mut canvas = fltk::widget::Widget::default()
            .with_pos(x, y)
            .with_size(w - 15, h);

        let widget_backend = state.clone();

        canvas.draw(move |w| {
            println!("DRAW!");
            let wd = widget_backend.borrow();
            let d = wd.doc.borrow();

            // Background
            fltk::draw::set_draw_color(fltk::enums::Color::from_rgb(40, 44, 52));
            fltk::draw::draw_rect_fill(
                w.x(),
                w.y(),
                w.width(),
                w.height(),
                fltk::enums::Color::BackGround,
            );

            // Setup Font
            fltk::draw::set_font(fltk::enums::Font::Courier, font_size);
            fltk::draw::set_draw_color(fltk::enums::Color::White);

            let total_lines = d.get_line_count();
            // Apply the scroll offset to determine which line to start drawing
            let start_line = wd.scroll_offset;
            let max_visible_lines = (w.height() / line_height) + 1;
            let end_line = std::cmp::min(total_lines, start_line + max_visible_lines as usize);

            for i in start_line..end_line {
                if let Some(text) = d.get_line(i) {
                    let draw_y = w.y() + ((i - start_line) as i32 * line_height);

                    // Line Numbers
                    fltk::draw::set_draw_color(fltk::enums::Color::Gray0);
                    fltk::draw::draw_text2(
                        &format!("{:3}", i + 1),
                        w.x(),
                        draw_y,
                        40,
                        line_height,
                        fltk::enums::Align::RightTop,
                    );

                    // Text Content
                    fltk::draw::set_draw_color(fltk::enums::Color::White);
                    fltk::draw::draw_text2(
                        &text,
                        w.x() + 45 + left_padding,
                        draw_y,
                        w.width() - 45,
                        line_height,
                        fltk::enums::Align::Left,
                    );
                }
            }
        });

        /*// Cursor blink timer (attached to the canvas)
        let mut timer_viewport = canvas.clone();
        let timer_backend = state.clone();

        fltk::app::add_timeout3(0.5, move |handle| {
            {
                let mut be = timer_backend.borrow_mut();
                be.cursor_visible = !be.cursor_visible;
            }
            timer_viewport.redraw();
            fltk::app::repeat_timeout3(0.5, handle);
        });*/

        // 3. Create the Scrollbar.
        // It's placed exactly where the canvas ends (x + w - 15).
        let mut scrollbar = fltk::valuator::Scrollbar::default()
            .with_pos(x + w - 15, y)
            .with_size(15, h);

        scrollbar.set_type(fltk::valuator::ScrollbarType::VerticalNice);
        scrollbar.set_color(fltk::enums::Color::from_rgb(200, 200, 200));
        scrollbar.set_selection_color(fltk::enums::Color::from_rgb(100, 100, 100));

        let visible_lines: f64 = h as f64 / line_height as f64;
        let max_scroll: f64 =
            (state.borrow().doc.borrow().get_line_count() as f64 - visible_lines).max(0.0);

        scrollbar.set_bounds(0.0, max_scroll);
        scrollbar.set_step(1.0, 1);

        let scroll_backend = state.clone();
        let mut scroll_viewport = canvas.clone();

        scrollbar.set_callback(move |s| {
            scroll_backend.borrow_mut().scroll_offset = s.value() as usize;
            scroll_viewport.redraw();
        });

        let handle_state = state.clone();
        let mut handle_scrollbar = scrollbar.clone();
        let handle_line_height = line_height;

        canvas.handle(move |c, event| {
            match event {
                fltk::enums::Event::MouseWheel => {
                    let dy = fltk::app::event_dy_value(); // Returns 1 (scroll down) or -1 (scroll up)

                    if dy == 0 {
                        return false;
                    }

                    let mut be = handle_state.borrow_mut();

                    let total_lines = be.doc.borrow().get_line_count() as i32;
                    let visible_lines = c.height() / handle_line_height;
                    let max_scroll = (total_lines - visible_lines).max(0);

                    // Calculate new offset and clamp it between 0 and max_scroll
                    let current_offset = be.scroll_offset as i32;
                    let mut new_offset = current_offset + (dy * 3); // Multiply by 3 to scroll faster

                    new_offset = new_offset.clamp(0, max_scroll);
                    handle_scrollbar.set_bounds(0.0, max_scroll as f64);

                    // If the scroll position actually changed, update everything
                    if be.scroll_offset != new_offset as usize {
                        be.scroll_offset = new_offset as usize;

                        // Update the physical scrollbar's position
                        handle_scrollbar.set_value(new_offset as f64);

                        // Force the canvas to redraw with the new text offset
                        c.redraw();
                    }
                    true // Tell FLTK we successfully handled this event
                }
                _ => false, // We didn't handle other events (like clicks) yet
            }
        });

        // 4. Close the Group.
        // This is crucial! It stops any future widgets (like your menu) from being added to it.
        grp.end();

        // Ensure the canvas stretches if the window resizes, but the scrollbar stays fixed.
        grp.resizable(&canvas);

        grp
    }
}
