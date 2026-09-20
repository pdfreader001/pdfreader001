pub mod document;
pub mod edit;
pub mod error;
pub mod pages;
pub mod render;
pub mod security;
pub mod watermark;

pub use document::pdfium;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            render::get_page_text,
            pages::rotate_pages,
            pages::delete_pages,
            pages::duplicate_pages,
            pages::insert_blank_page,
            pages::reorder_pages,
            pages::extract_pages,
            pages::merge_documents,
            pages::split_document,
            pages::get_bookmarks,
            watermark::add_text_watermark,
            watermark::add_image_watermark,
            edit::list_annotations,
            edit::add_annotation,
            edit::delete_annotation,
            edit::clear_annotations,
            security::get_security_status,
            security::export_plain_copy,
            security::reload_plain
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
