#![forbid(unsafe_code)]

mod commands;

pub fn run() -> tauri::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::get_health,
            commands::send_chat_message,
            commands::cancel_chat_message,
            commands::list_chat_sessions,
            commands::list_chat_messages,
            commands::list_activity,
            commands::list_memory,
            commands::get_memory_history,
            commands::update_memory_state,
            commands::restore_memory_revision,
            commands::list_providers,
            commands::upsert_provider,
            commands::remove_provider,
            commands::test_provider,
            commands::list_models,
            commands::upsert_model,
            commands::get_application_settings,
            commands::update_application_settings,
            commands::get_usage_stats,
            commands::get_diagnostics,
            commands::clear_operational_logs,
            commands::submit_response_feedback,
            commands::confirm_action,
            commands::preview_ai_request,
        ])
        .run(tauri::generate_context!())
}
