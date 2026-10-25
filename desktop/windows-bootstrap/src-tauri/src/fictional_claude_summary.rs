// Gate ABF134F: explicit FICTIONAL demonstration only. Not real consumption, and no
// production event/database contract. Takes no argument and reads nothing at run time.
#[tauri::command]
pub(crate) fn abf_get_fictional_claude_summary() -> String {
    abf_collection_boundary::claude_demo::fictional_demo_payload_json()
}
