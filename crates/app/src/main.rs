use fltk::prelude::{GroupExt, MenuExt, WidgetExt};

pub fn main() {
    let text_editor = ui::TextEditor::new(
        0,
        0,
        100,
        100,
        std::rc::Rc::new(std::cell::RefCell::new(
            editor_state::document::Document::new(editor_core::text::TextBuffer::new().unwrap()),
        )),
    );

    let app = fltk::app::App::default();
    let mut win = fltk::window::Window::default()
        .with_size(400, 300)
        .with_label("MyNotes");
    let mut menu = fltk::menu::MenuBar::default().with_size(800, 30);

    menu.add(
        "File/Open...",
        fltk::enums::Shortcut::Ctrl | 'o',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(file_path) =
                fltk::dialog::file_chooser("Open File", "*.{txt,rs,md,log}", ".", false)
            {
                println!("Open File: {}", file_path);

                fltk::app::redraw();
            }
        },
    );

    win.end();
    win.make_resizable(true);
    win.show();

    app.run().unwrap();
}
