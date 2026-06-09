pub mod parser;
use parser::ScriptEntry;
use std::path::Path;
use walkdir::WalkDir;
use std::sync::atomic::{AtomicUsize, Ordering};

#[cfg(target_os = "windows")]
extern "system" {
    fn SetThreadExecutionState(es_flags: u32) -> u32;
}

#[cfg(target_os = "windows")]
const ES_CONTINUOUS: u32 = 0x80000000;
#[cfg(target_os = "windows")]
const ES_SYSTEM_REQUIRED: u32 = 0x00000001;
#[cfg(target_os = "windows")]
const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

static PREVENT_SLEEP_COUNT: AtomicUsize = AtomicUsize::new(0);

use std::process::{Command, Child};
use std::sync::{Mutex, OnceLock};
use std::io::Write;
use tauri::Manager;

#[cfg(target_os = "macos")]
static CAFFEINATE_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

#[cfg(target_os = "macos")]
fn get_caffeinate_mutex() -> &'static Mutex<Option<Child>> {
    CAFFEINATE_CHILD.get_or_init(|| Mutex::new(None))
}

static LLAMA_SERVER_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

fn get_llama_server_mutex() -> &'static Mutex<Option<Child>> {
    LLAMA_SERVER_CHILD.get_or_init(|| Mutex::new(None))
}

#[tauri::command]
fn prevent_sleep() {
    #[cfg(target_os = "windows")]
    {
        if PREVENT_SLEEP_COUNT.fetch_add(1, Ordering::SeqCst) == 0 {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            }
            println!("절전 모드 및 디스플레이 오프 방지 활성화 (Windows)");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if PREVENT_SLEEP_COUNT.fetch_add(1, Ordering::SeqCst) == 0 {
            let mutex = get_caffeinate_mutex();
            let mut guard = mutex.lock().unwrap();
            if guard.is_none() {
                let current_pid = std::process::id();
                match Command::new("caffeinate")
                    .args(&["-d", "-i", "-w", &current_pid.to_string()])
                    .spawn() 
                {
                    Ok(child) => {
                        *guard = Some(child);
                        println!("절전 모드 및 디스플레이 오프 방지 활성화 (macOS - caffeinate pid: {})", current_pid);
                    }
                    Err(e) => {
                        eprintln!("caffeinate 실행 실패: {}", e);
                    }
                }
            }
        }
    }
}

#[tauri::command]
fn allow_sleep() {
    #[cfg(target_os = "windows")]
    {
        let current = PREVENT_SLEEP_COUNT.load(Ordering::SeqCst);
        if current > 0 {
            if PREVENT_SLEEP_COUNT.fetch_sub(1, Ordering::SeqCst) == 1 {
                unsafe {
                    SetThreadExecutionState(ES_CONTINUOUS);
                }
                println!("절전 모드 설정 기본값으로 복구 (Windows)");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let current = PREVENT_SLEEP_COUNT.load(Ordering::SeqCst);
        if current > 0 {
            if PREVENT_SLEEP_COUNT.fetch_sub(1, Ordering::SeqCst) == 1 {
                let mutex = get_caffeinate_mutex();
                let mut guard = mutex.lock().unwrap();
                if let Some(mut child) = guard.take() {
                    let _ = child.kill();
                    println!("절전 모드 설정 기본값으로 복구 (macOS - caffeinate 종료)");
                }
            }
        }
    }
}


#[tauri::command]
fn parse_rvdata(path: String) -> Result<Vec<ScriptEntry>, String> {
    parser::parse_rvdata(&path)
}

#[tauri::command]
fn save_rvdata(original_path: String, new_path: String, updated_scripts: Vec<ScriptEntry>) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&new_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    parser::save_rvdata(&original_path, &new_path, updated_scripts)
}

#[tauri::command]
fn get_images_in_folder(folder_path: String) -> Result<Vec<String>, String> {
    let mut images = Vec::new();
    for entry in WalkDir::new(&folder_path).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext = ext.to_lowercase();
                if ext == "png" || ext == "jpg" || ext == "jpeg" || ext == "bmp" {
                    images.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    Ok(images)
}

#[tauri::command]
fn read_image_file(path: String) -> Result<Vec<u8>, String> {
    std::fs::read(&path).map_err(|e| e.to_string())
}

use image::Rgba;
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use ab_glyph::FontRef;
use serde::Deserialize;

#[derive(Deserialize)]
struct OcrRegion {
    ko_text: String,
    x: f32, // 0~100 %
    y: f32, // 0~100 %
    w: f32,
    h: f32,
}

#[tauri::command]
fn draw_and_save_image(original_path: String, output_path: String, regions: Vec<OcrRegion>) -> Result<(), String> {
    let mut img = image::open(&original_path).map_err(|e| e.to_string())?.to_rgba8();
    let (width, height) = img.dimensions();

    let font_paths = vec![
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\malgun.ttf".to_string(),
        #[cfg(target_os = "windows")]
        "C:\\Windows\\Fonts\\malgunbd.ttf".to_string(),
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/AppleGothic.ttf".to_string(),
        #[cfg(target_os = "macos")]
        "/System/Library/Fonts/Supplemental/AppleMyungjo.ttf".to_string(),
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf".to_string(),
        #[cfg(target_os = "linux")]
        "/usr/share/fonts/nanum/NanumGothic.ttf".to_string(),
    ];

    let mut font_bytes = None;
    for path in &font_paths {
        if let Ok(bytes) = std::fs::read(path) {
            font_bytes = Some(bytes);
            break;
        }
    }

    let font_bytes = font_bytes.ok_or_else(|| {
        format!(
            "시스템 폰트를 찾을 수 없습니다. 시도한 경로: {:?}",
            font_paths
        )
    })?;
    let font = FontRef::try_from_slice(&font_bytes).map_err(|e| e.to_string())?;

    for r in regions {
        // 여유(패딩)를 약간 주어 원본 일본어를 더 잘 가리도록 함
        let pad_w = (width as f32 * 0.02) as i32;
        let pad_h = (height as f32 * 0.02) as i32;
        let px = ((r.x / 100.0 * width as f32) as i32 - pad_w).max(0);
        let py = ((r.y / 100.0 * height as f32) as i32 - pad_h).max(0);
        let pw = ((r.w / 100.0 * width as f32) as u32 + (pad_w * 2) as u32).max(1);
        let ph = ((r.h / 100.0 * height as f32) as u32 + (pad_h * 2) as u32).max(1);

        // 검정색 배경 박스 (알파 200)
        let rect = Rect::at(px, py).of_size(pw, ph);
        draw_filled_rect_mut(&mut img, rect, Rgba([0, 0, 0, 200]));

        // 글자 크기를 박스 높이의 70%로 설정하고, 너비가 너무 작으면 거기에 맞춤
        let mut scale = (ph as f32 * 0.7).max(16.0);
        let text_len = r.ko_text.chars().count().max(1) as f32;
        // 박스 너비보다 글씨가 삐져나가지 않도록 스케일 조정
        if scale * text_len > pw as f32 {
            scale = (pw as f32 / text_len).max(12.0);
        }
        let ab_scale = ab_glyph::PxScale::from(scale);
        
        let tx = px + (pad_w / 2).max(2);
        let ty = py + (pad_h / 2).max(2);

        // 검은색 외곽선(Stroke)을 그리기 위해 상하좌우대각선 8방향으로 1~2px 밀어서 렌더링
        let outline_color = Rgba([0, 0, 0, 255]);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx != 0 || dy != 0 {
                    draw_text_mut(&mut img, outline_color, tx + dx, ty + dy, ab_scale, &font, &r.ko_text);
                }
            }
        }
        
        // 텍스트 그리기 (흰색 글씨)
        draw_text_mut(&mut img, Rgba([255, 255, 255, 255]), tx, ty, ab_scale, &font, &r.ko_text);
    }

    if let Some(parent) = Path::new(&output_path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    img.save(&output_path).map_err(|e| e.to_string())?;

    Ok(())
}

use tauri::Emitter;

#[derive(Clone, serde::Serialize)]
struct ProgressPayload {
    file_name: String,
    current: usize,
    total: usize,
}

#[tauri::command]
async fn decompile_rgss3a(
    app: tauri::AppHandle,
    input_path: String,
    output_path: String,
) -> Result<(), String> {
    let mut archive_content = std::fs::read(&input_path)
        .map_err(|e| format!("아카이브 파일을 읽는 데 실패했습니다: {}", e))?;

    let mut decrypter = rpgmad_lib::Decrypter::new();
    let decrypted_entries: Vec<_> = decrypter.decrypt(&mut archive_content)
        .map_err(|e| format!("복호화 오류가 발생했습니다: {}", e))?
        .collect();

    let total = decrypted_entries.len();
    let output_dir = std::path::PathBuf::from(&output_path);

    for (index, entry) in decrypted_entries.into_iter().enumerate() {
        let path_str = String::from_utf8_lossy(entry.path).replace('\\', "/");
        let dest_path = output_dir.join(&path_str);

        if let Some(parent) = dest_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("폴더 생성 실패: {}", e))?;
        }

        std::fs::write(&dest_path, entry.data)
            .map_err(|e| format!("파일 쓰기 실패 ({}): {}", path_str, e))?;

        let _ = app.emit("decompile-progress", ProgressPayload {
            file_name: path_str,
            current: index + 1,
            total,
        });
    }

    Ok(())
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn check_file_exists(path: String) -> bool {
    std::path::Path::new(&path).exists()
}

#[tauri::command]
fn get_rvdata_in_folder(folder_path: String) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    let entries = std::fs::read_dir(&folder_path).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext = ext.to_lowercase();
                if ext == "rvdata" || ext == "rvdata2" {
                    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    if !file_name.contains("_staged") && !file_name.contains("_translated") {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

#[tauri::command]
fn save_staged_json(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn read_staged_json(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_directory(path: String) -> bool {
    std::path::Path::new(&path).is_dir()
}

#[tauri::command]
fn copy_file(src: String, dest: String) -> Result<(), String> {
    if let Some(parent) = std::path::Path::new(&dest).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone, serde::Serialize)]
struct DownloadProgressPayload {
    model_id: String,
    downloaded: u64,
    total: u64,
    percentage: f64,
}

#[tauri::command]
fn check_model_exists(app: tauri::AppHandle, model_id: String) -> bool {
    if let Ok(app_dir) = app.path().app_data_dir() {
        let model_path = app_dir.join("models").join(format!("{}-Q4_K_M.gguf", model_id));
        model_path.exists()
    } else {
        false
    }
}

#[tauri::command]
fn download_model(app: tauri::AppHandle, model_id: String, url: String) -> Result<(), String> {
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let client = reqwest::blocking::Client::builder()
                .build()
                .map_err(|e| e.to_string())?;
            let mut response = client.get(&url)
                .send()
                .map_err(|e| e.to_string())?;
            
            if !response.status().is_success() {
                return Err(format!("HTTP error: {}", response.status()));
            }

            let total_size = response.content_length().unwrap_or(0);
            
            let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let models_dir = app_dir.join("models");
            std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
            
            let filename = format!("{}-Q4_K_M.gguf", model_id);
            let dest_path = models_dir.join(&filename);
            let tmp_path = models_dir.join(format!("{}.tmp", filename));
            
            let mut dest_file = std::fs::File::create(&tmp_path).map_err(|e| e.to_string())?;
            
            let mut buffer = vec![0; 8192];
            let mut downloaded = 0;
            
            loop {
                use std::io::Read;
                let bytes_read = response.read(&mut buffer).map_err(|e| e.to_string())?;
                if bytes_read == 0 {
                    break;
                }
                dest_file.write_all(&buffer[..bytes_read]).map_err(|e| e.to_string())?;
                downloaded += bytes_read as u64;
                
                let percentage = if total_size > 0 {
                    (downloaded as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                };
                
                let _ = app.emit("download-progress", DownloadProgressPayload {
                    model_id: model_id.clone(),
                    downloaded,
                    total: total_size,
                    percentage,
                });
            }
            
            // Rename tmp file to final file
            std::fs::rename(tmp_path, dest_path).map_err(|e| e.to_string())?;
            
            let _ = app.emit("download-complete", model_id.clone());
            Ok(())
        })();
        
        if let Err(err) = result {
            let _ = app.emit("download-error", (model_id, err));
        }
    });
    
    Ok(())
}

fn kill_existing_llama_servers() {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .args(&["/F", "/IM", "llama-server.exe"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let _ = Command::new("pkill")
            .arg("-x")
            .arg("llama-server")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = Command::new("killall")
            .arg("llama-server")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

#[tauri::command]
fn start_llama_server(app: tauri::AppHandle, model_id: String, custom_path: Option<String>) -> Result<(), String> {
    let mutex = get_llama_server_mutex();
    let mut guard = mutex.lock().unwrap();
    
    // Kill existing child process if running
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
    }

    // Kill any orphaned llama-server processes from previous sessions
    kill_existing_llama_servers();
    std::thread::sleep(std::time::Duration::from_millis(500));
    
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let model_path = app_dir.join("models").join(format!("{}-Q4_K_M.gguf", model_id));
    
    if !model_path.exists() {
        return Err("Model file does not exist. Please download it first.".to_string());
    }
    
    let exe_name = if cfg!(target_os = "windows") { "llama-server.exe" } else { "llama-server" };
    
    let mut server_path = custom_path.unwrap_or_default();
    if server_path.is_empty() {
        // 1. 앱 실행 파일과 같은 폴더에서 찾기
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let local_path = parent.join(exe_name);
                if local_path.exists() {
                    server_path = local_path.to_string_lossy().to_string();
                }
            }
        }
    }
    
    // 2. 지정되지 않았거나 같은 폴더에 없으면 시스템 PATH에서 찾기
    if server_path.is_empty() {
        server_path = exe_name.to_string();
    }
    
    println!("Starting llama-server: {} -m {:?}", server_path, model_path);

    // 로그 파일 설정
    let log_dir = app_dir.join("logs");
    std::fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let log_path = log_dir.join("llama_server.log");
    
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {}", e))?;
        
    let log_file_err = log_file.try_clone().map_err(|e| e.to_string())?;
    
    let model_path_str = model_path.to_string_lossy().to_string();
    let mut args = vec![
        "-m", &model_path_str,
        "--port", "8080",
        "-c", "2048",
        "-np", "1",
    ];

    if cfg!(target_os = "windows") {
        // Windows (Vulkan 가속용 최적화 옵션)
        args.push("-ngl");
        args.push("99");
        args.push("-fa");
        args.push("on");
    } else if cfg!(target_os = "macos") {
        // macOS (Apple Silicon Metal 가속용 최적화 옵션)
        args.push("-ngl");
        args.push("99");
    } else {
        // 기타 OS
        args.push("-ngl");
        args.push("99");
    }

    let mut child = Command::new(&server_path)
        .args(&args)
        .stdout(log_file)
        .stderr(log_file_err)
        .spawn()
        .map_err(|e| format!("Failed to start llama-server. Error: {}", e))?;
        
    // 1초간 대기하여 바로 종료(크래시)되는지 확인
    std::thread::sleep(std::time::Duration::from_millis(1000));
    match child.try_wait() {
        Ok(Some(status)) => {
            let log_content = std::fs::read_to_string(&log_path)
                .unwrap_or_else(|_| "Failed to read llama_server.log".to_string());
            
            // Limit log lines to avoid massive errors
            let log_lines: Vec<&str> = log_content.lines().collect();
            let last_lines = if log_lines.len() > 30 {
                log_lines[log_lines.len() - 30..].join("\n")
            } else {
                log_content.clone()
            };
            
            return Err(format!(
                "llama-server exited immediately with status: {}.\nLog output:\n{}",
                status, last_lines
            ));
        }
        Ok(None) => {}
        Err(e) => {
            return Err(format!("Error checking child process status: {}", e));
        }
    }
        
    *guard = Some(child);
    Ok(())
}

#[tauri::command]
fn stop_llama_server() {
    let mutex = get_llama_server_mutex();
    let mut guard = mutex.lock().unwrap();
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        println!("llama-server stopped.");
    }
}

#[tauri::command]
fn check_llama_server_availability(custom_path: Option<String>) -> bool {
    let exe_name = if cfg!(target_os = "windows") { "llama-server.exe" } else { "llama-server" };
    
    let server_path = custom_path.unwrap_or_default();
    if server_path.is_empty() {
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let local_path = parent.join(exe_name);
                if local_path.exists() {
                    return true;
                }
            }
        }
    } else {
        return std::path::Path::new(&server_path).exists();
    }
    
    if let Ok(path_var) = std::env::var("PATH") {
        for path in std::env::split_paths(&path_var) {
            let exe_path = path.join(exe_name);
            if exe_path.exists() {
                return true;
            }
        }
    }
    
    false
}

#[tauri::command]
fn delete_model(app: tauri::AppHandle, model_id: String) -> Result<(), String> {
    let app_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let model_path = app_dir.join("models").join(format!("{}-Q4_K_M.gguf", model_id));
    if model_path.exists() {
        std::fs::remove_file(model_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                stop_llama_server();
            }
        })
        .invoke_handler(tauri::generate_handler![
            parse_rvdata, 
            save_rvdata, 
            get_images_in_folder,
            read_image_file,
            draw_and_save_image,
            decompile_rgss3a,
            open_folder,
            prevent_sleep,
            allow_sleep,
            check_file_exists,
            get_rvdata_in_folder,
            save_staged_json,
            read_staged_json,
            is_directory,
            copy_file,
            check_model_exists,
            download_model,
            start_llama_server,
            stop_llama_server,
            delete_model,
            check_llama_server_availability
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                stop_llama_server();
            }
        });
}
