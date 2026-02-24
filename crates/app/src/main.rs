use fltk::prelude::{GroupExt, MenuExt, WidgetExt};

pub fn main() {
    let app = fltk::app::App::default();
    let mut win = fltk::window::Window::default()
        .with_size(400, 300)
        .with_label("MyNotes");
    let backend = std::rc::Rc::new(std::cell::RefCell::new(
        editor_state::document::Document::new(editor_core::text::TextBuffer::new().unwrap()),
    ));
    let mut text_editor = ui::TextEditor::new_editor(0, 30, 400, 270, backend.clone());
    let mut menu = fltk::menu::MenuBar::default().with_size(800, 30);
    let menu_backend = backend.clone();

    menu.add(
        "File/Open...",
        fltk::enums::Shortcut::Ctrl | 'o',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(file_path) =
                fltk::dialog::file_chooser("Open File", "*.{txt,rs,md,log}", ".", false)
            {
                println!("Open File: {}", file_path);
                menu_backend.borrow_mut().open_file(file_path).unwrap();

                fltk::app::redraw();
            }
        },
    );

    win.end();
    win.make_resizable(true);
    win.show();
    text_editor.show();

    app.run().unwrap();
}
