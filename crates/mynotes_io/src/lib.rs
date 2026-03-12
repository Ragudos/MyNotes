use std::{
    fs::{OpenOptions, create_dir_all},
    io::{self, ErrorKind},
    path::PathBuf,
};

use directories::ProjectDirs;

pub mod enums;
pub mod mmap;
pub mod save;
pub mod swap_manager;

pub fn get_app_dir() -> Option<PathBuf> {
    if let Some(proj_dirs) = ProjectDirs::from("com", "ragudos", "MyNotes") {
        let app_dir = proj_dirs.data_dir().to_path_buf();

        if !app_dir.exists() {
            if let Err(e) = create_dir_all(&app_dir) {
                eprintln!("Failed to create app directory: {}", e);
                return None;
            }
        }

        return Some(app_dir);
    }

    None
}

pub fn get_unsaved_dir() -> Option<PathBuf> {
    let mut dir = get_app_dir()?;

    dir.push("unsaved_sessions");

    if !dir.exists() {
        if let Err(e) = create_dir_all(&dir) {
            eprintln!("Failed to create unsaved directory: {}", e);
            return None;
        }
    }

    Some(dir)
}

pub fn create_next_untitled_file() -> io::Result<(usize, PathBuf)> {
    let base_dir = get_unsaved_dir()
        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "App dir not found"))?;
    let mut id = 1;

    loop {
        let mut path = base_dir.clone();

        path.push(format!("untitled_{}.swp", id));

        // `create_new(true)` is the magic here.
        // It strictly requires that the file DOES NOT exist.
        let result = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path);

        match result {
            Ok(_) => {
                // Success! We claimed this ID before anyone else could.
                return Ok((id, path));
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                // This ID is taken (either by a crashed session or another editor window).
                // Just bump the ID and try the next one.
                id += 1;

                continue;
            }
            Err(e) => {
                // A legitimate permissions or hardware error occurred.
                return Err(e);
            }
        }
    }
}
