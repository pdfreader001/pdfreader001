pub mod document;
pub mod error;
pub mod render;

pub use document::pdfium;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(document::AppState::default())
        .invoke_handler(tauri::generate_handler![
            document::open_document,
            document::close_document,
            document::get_metadata,
            document::save_document,
            document::undo_document,
            document::can_undo,
            render::render_page,
            render::render_thumbnail,
            render::get_page_text
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
