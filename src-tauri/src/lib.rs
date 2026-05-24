pub mod parser;

use parser::ScriptEntry;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn parse_rvdata(path: String) -> Result<Vec<ScriptEntry>, String> {
    parser::parse_rvdata(&path)
}

#[tauri::command]
fn save_rvdata(original_path: String, new_path: String, updated_scripts: Vec<ScriptEntry>) -> Result<(), String> {
    parser::save_rvdata(&original_path, &new_path, updated_scripts)
}

#[tauri::command]
async fn translate_ollama(prompt: String) -> Result<String, String> {
    let client = reqwest::Client::new();
    let body = serde_json::json!({
        "model": "gemma4:e4b",
        "prompt": prompt,
        "stream": false
    });

    let res = client.post("http://127.0.0.1:11434/api/generate")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Reqwest failed: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("Ollama HTTP Error: {}", res.status()));
    }

    let json: serde_json::Value = res.json().await.map_err(|e| format!("JSON parse error: {}", e))?;
    
    if let Some(resp) = json.get("response").and_then(|v| v.as_str()) {
        Ok(resp.to_string())
    } else {
        Err("Missing response field in Ollama output".into())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![parse_rvdata, save_rvdata, translate_ollama])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
