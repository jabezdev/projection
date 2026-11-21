//! Operator UI commands: engine control, the song and service library, and
//! media files. Output and display commands live in `displays`, settings in
//! `settings`.




mod media;
mod missing_media;
mod pdf;
mod services;
mod songs;
mod system;
mod transport;

pub use media::*;
pub use missing_media::*;
pub use pdf::*;
pub use services::*;
pub use songs::*;
pub use system::*;
pub use transport::*;

pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        crate::displays::get_output_sizes,
        crate::displays::get_output_preview,
        crate::displays::set_output_monitor,
        crate::displays::preview_output_style,
        crate::displays::get_output_displays,
        crate::displays::set_output_display,
        crate::displays::set_output_shown,
        get_startup_warnings,
        open_logs_folder,
        get_app_info,
        crate::settings::get_video_decoding,
        crate::settings::set_video_decoder,
        crate::settings::get_media_autoplay,
        crate::settings::set_media_autoplay,
        crate::capture::get_capture_methods,
        crate::capture::set_capture_method,
        crate::capture::list_capture_sources,
        crate::capture::pick_capture_source,
        crate::capture::get_capture_status,
        crate::updates::get_update_status,
        crate::updates::check_for_update,
        crate::updates::install_update,
        crate::updates::reinstall_version,
        crate::remote::get_remote_status,
        crate::remote::set_remote_settings,
        crate::remote::start_pairing,
        crate::remote::stop_pairing,
        crate::remote::remove_remote_device,
        export_backup,
        open_backups_folder,
        list_missing_media,
        relink_media,
        import_openlyrics,
        crate::input::get_input_bindings,
        crate::input::set_input_bindings,
        crate::input::reset_input_bindings,
        crate::input::trigger_action,
        crate::input::learn_input,
        crate::input::get_input_devices,
        crate::settings::get_output_state,
        crate::settings::set_output_state,
        resolve_media_path,
        video_thumbnail,
        choose_presentation_images,
        choose_pdf,
        read_chosen_pdf,
        save_slide_image,
        play,
        pause,
        stop,
        nav_next,
        nav_prev,
        cue,
        engine_command,
        go_to_item,
        go_to_step,
        seek,
        set_active_text,
        set_background,
        set_display,
        get_engine_snapshot,
        list_projects,
        create_project,
        get_project,
        load_project,
        save_project,
        list_songs,
        create_song,
        get_song,
        update_song,
        delete_project,
        delete_song,
        delete_media,
        rename_media,
        import_media_files,
        list_media,
        create_media,
        choose_media_file
    ]
}
