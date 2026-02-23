use fltk::prelude::*;

pub struct TextEditor {
    widget: fltk::widget::Widget,
}

impl TextEditor {
    pub fn new(
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        doc: std::rc::Rc<std::cell::RefCell<editor_state::document::Document>>,
    ) -> Self {
        let mut widget = fltk::widget::Widget::new(x, y, w, h, "TextEditor");

        let line_height = 20;
        let font_size = 14;
        let left_padding = 6;

        widget.draw(move |w| {
            let d = doc.borrow();

            fltk::draw::set_draw_color(fltk::enums::Color::from_rgb(40, 44, 52)); // One Dark-ish theme
            fltk::draw::draw_rect_fill(
                w.x(),
                w.y(),
                w.width(),
                w.height(),
                fltk::enums::Color::BackGround,
            );
            // 2. Setup Font
            fltk::draw::set_font(fltk::enums::Font::Courier, font_size);
            fltk::draw::set_draw_color(fltk::enums::Color::White);

            let total_lines = d.get_line_count();
            let start_line = 0;
            let max_visible_lines = (w.height() / line_height) + 1;
            let end_line = std::cmp::min(total_lines, start_line + max_visible_lines as usize);

            for i in start_line..end_line {
                if let Some(text) = d.get_line(i) {
                    let draw_y = w.y() + ((i - start_line) as i32 * line_height);

                    fltk::draw::set_draw_color(fltk::enums::Color::Gray0);
                    fltk::draw::draw_text2(
                        &format!("{:3}", i + 1),
                        w.x(),
                        draw_y,
                        40,
                        line_height,
                        fltk::enums::Align::RightTop,
                    );
                    fltk::draw::set_draw_color(fltk::enums::Color::White);
                    fltk::draw::draw_text2(
                        &text,
                        w.x() + 45,
                        draw_y,
                        w.width() - 45,
                        line_height,
                        fltk::enums::Align::Left,
                    );
                }
            }
        });

        Self { widget }
    }
}
