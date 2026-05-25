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

#[tauri::command]
fn prevent_sleep() {
    #[cfg(target_os = "windows")]
    {
        if PREVENT_SLEEP_COUNT.fetch_add(1, Ordering::SeqCst) == 0 {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            }
            println!("절전 모드 및 디스플레이 오프 방지 활성화");
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
                println!("절전 모드 설정 기본값으로 복구");
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

use image::{Rgba, RgbaImage};
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
            is_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
