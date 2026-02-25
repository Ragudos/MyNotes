use fltk::prelude::{GroupExt, MenuExt, WidgetExt};

pub fn main() {
    let app = fltk::app::App::default();
    let mut win = fltk::window::Window::default()
        .with_size(400, 300)
        .with_label("MyNotes");
    let backend = std::rc::Rc::new(std::cell::RefCell::new(
        editor_state::document::Document::new(editor_core::text::TextBuffer::new().unwrap()),
    ));
    let mut text_editor = ui::TextEditor::new(0, 30, 400, 270, backend.clone());
    let text_editor_state = text_editor.state.clone();
    let mut menu = fltk::menu::MenuBar::default().with_size(400, 30);
    let menu_backend = backend.clone();

    win.resizable(&text_editor.group);

    menu.add(
        "File/Open...",
        fltk::enums::Shortcut::Ctrl | 'o',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            if let Some(file_path) =
                fltk::dialog::file_chooser("Open File", "*.{txt,rs,md,log}", ".", false)
            {
                menu_backend.borrow_mut().open_file(file_path).unwrap();
                text_editor.on_content_changed();

                fltk::app::redraw();
            }
        },
    );

    menu.add(
        "File/Save...",
        fltk::enums::Shortcut::Ctrl | 's',
        fltk::menu::MenuFlag::Normal,
        move |_| {
            // 1. DANGEROUS ZONE AVERTED: Just check if we need a path, then DROP the borrow immediately.
            let needs_path = {
                let text_state = text_editor_state.borrow();
                let d = text_state.doc.borrow();
                d.text_buffer.path().is_none()
            }; // The `text_state` and `d` borrows end right here!

            let selected_path = if needs_path {
                // 2. SAFE ZONE: We hold no borrows here. The timer can happily fire in the background.
                let mut dialog = fltk::dialog::NativeFileChooser::new(
                    fltk::dialog::NativeFileChooserType::BrowseSaveFile,
                );

                dialog.set_title("Save File As...");
                // FIX: Changed /t to \t so the native dialog parses the categories correctly
                dialog.set_filter("Text\t*.txt\nRust\t*.rs\nMarkdown\t*.md\nAll\t*.*");

                dialog.show();

                let path = dialog.filename();

                if path.as_os_str().is_empty() {
                    return; // User cancelled
                }
                Some(path)
            } else {
                None
            };

            // 3. RE-BORROW: We have our path (or know we don't need one), so borrow again just to save.
            let text_state = text_editor_state.borrow_mut();
            let mut d = text_state.doc.borrow_mut();

            if let Some(path) = selected_path {
                match d.text_buffer.save_as(path.as_path()) {
                    Ok(_) => println!("Success! Saved to {:?}", path),
                    Err(err) => println!("Error saving file: {:?}", err),
                };
            } else {
                match d.text_buffer.save() {
                    Ok(_) => println!("Success! Saved to existing path."),
                    Err(err) => println!("Error saving file: {:?}", err),
                }
            }
        },
    );

    win.end();
    win.show();

    app.run().unwrap();
}
