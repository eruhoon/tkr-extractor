use alox_48::{Value, Symbol, RbFields};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{Read, Write};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ScriptEntry {
    pub id: i64,
    pub title: String,
    pub code: String,
}

// ==========================================
// 1. 공통 헬퍼 함수 정의
// ==========================================

fn get_fields(val: &Value) -> Result<&RbFields, String> {
    match val {
        Value::Object(obj) => Ok(&obj.fields),
        Value::Instance(inst) => {
            if let Value::Object(obj) = &*inst.value {
                Ok(&obj.fields)
            } else {
                Ok(&inst.fields)
            }
        }
        _ => Err("Value does not have fields".to_string()),
    }
}

fn get_fields_mut(val: &mut Value) -> Result<&mut RbFields, String> {
    match val {
        Value::Object(obj) => Ok(&mut obj.fields),
        Value::Instance(inst) => {
            if let Value::Object(obj) = &mut *inst.value {
                Ok(&mut obj.fields)
            } else {
                Ok(&mut inst.fields)
            }
        }
        _ => Err("Value does not have fields".to_string()),
    }
}

fn get_string_value(val: &Value) -> Option<String> {
    match val {
        Value::String(s) => Some(s.to_string_lossy().into_owned()),
        Value::Instance(inst) => {
            if let Value::String(s) = &*inst.value {
                Some(s.to_string_lossy().into_owned())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn set_string_value(val: &mut Value, text: String) -> Result<(), String> {
    match val {
        Value::String(s) => {
            s.data = text.into_bytes();
            Ok(())
        }
        Value::Instance(inst) => {
            if let Value::String(s) = &mut *inst.value {
                s.data = text.into_bytes();
                Ok(())
            } else {
                Err("Instance value is not a String".to_string())
            }
        }
        _ => Err("Value is not a String or Instance".to_string()),
    }
}

fn has_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let cp = c as u32;
        // 히라가나, 가타카나, CJK 통합 한자 범위 체크
        (0x3040..=0x309F).contains(&cp) || (0x30A0..=0x30FF).contains(&cp) || (0x4E00..=0x9FFF).contains(&cp)
    })
}

// ==========================================
// 2. 통합 진입점 정의 (파일명 자동 감지)
// ==========================================

pub fn parse_rvdata(path: &str) -> Result<Vec<ScriptEntry>, String> {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let value: Value = alox_48::from_bytes(&buffer).map_err(|e| format!("Marshal Error: {:?}", e))?;

    if filename.starts_with("scripts") {
        parse_scripts(value)
    } else if filename.starts_with("map") {
        parse_map_events(value)
    } else if filename.starts_with("commonevents") {
        parse_common_events(value)
    } else if filename.starts_with("items") {
        parse_database_items(value, "Item")
    } else if filename.starts_with("skills") {
        parse_database_items(value, "Skill")
    } else if filename.starts_with("actors") {
        parse_database_items(value, "Actor")
    } else if filename.starts_with("weapons") {
        parse_database_items(value, "Weapon")
    } else if filename.starts_with("armors") {
        parse_database_items(value, "Armor")
    } else if filename.starts_with("enemies") {
        parse_database_items(value, "Enemy")
    } else {
        // 미지원 파일도 일단 데이터베이스 형태로 읽어본다 (유연성 확보)
        parse_database_items(value, "Data")
    }
}

pub fn save_rvdata(original_path: &str, new_path: &str, updated_scripts: Vec<ScriptEntry>) -> Result<(), String> {
    let filename = std::path::Path::new(original_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    let mut file = File::open(original_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let mut value: Value = alox_48::from_bytes(&buffer).map_err(|e| format!("Marshal Error: {:?}", e))?;

    if filename.starts_with("scripts") {
        save_scripts(&mut value, updated_scripts)?;
    } else if filename.starts_with("map") {
        save_map_events(&mut value, updated_scripts)?;
    } else if filename.starts_with("commonevents") {
        save_common_events(&mut value, updated_scripts)?;
    } else if filename.starts_with("items")
        || filename.starts_with("skills")
        || filename.starts_with("actors")
        || filename.starts_with("weapons")
        || filename.starts_with("armors")
        || filename.starts_with("enemies")
    {
        save_database_items(&mut value, updated_scripts)?;
    } else {
        save_database_items(&mut value, updated_scripts)?;
    }

    let out_bytes = alox_48::to_bytes(&value).map_err(|e| format!("Marshal Error: {:?}", e))?;
    let mut out_file = File::create(new_path).map_err(|e| e.to_string())?;
    out_file.write_all(&out_bytes).map_err(|e| e.to_string())?;

    Ok(())
}

// ==========================================
// 3. 개별 파일 파서 구현들
// ==========================================

// Scripts.rvdata / Scripts.rvdata2 파서
fn parse_scripts(value: Value) -> Result<Vec<ScriptEntry>, String> {
    let mut scripts = Vec::new();

    if let Value::Array(arr) = value {
        for item in arr.into_iter() {
            if let Value::Array(script_tuple) = item {
                if script_tuple.len() >= 3 {
                    let id = match &script_tuple[0] {
                        Value::Integer(i) => *i as i64,
                        _ => 0,
                    };

                    let title = match &script_tuple[1] {
                        Value::String(s) => s.to_string_lossy().into_owned(),
                        Value::Instance(inst) => {
                            if let Value::String(s) = &*inst.value {
                                s.to_string_lossy().into_owned()
                            } else {
                                "Unknown Title".to_string()
                            }
                        },
                        _ => "Unknown Title".to_string(),
                    };

                    let compressed_data = match &script_tuple[2] {
                        Value::String(s) => Some(s.clone()),
                        Value::Instance(inst) => {
                            if let Value::String(s) = &*inst.value {
                                Some(s.clone())
                            } else { None }
                        },
                        _ => None,
                    };

                    let code = if let Some(data) = compressed_data {
                        let mut decoder = ZlibDecoder::new(data.as_slice());
                        let mut decompressed = String::new();
                        match decoder.read_to_string(&mut decompressed) {
                            Ok(_) => decompressed,
                            Err(_) => {
                                let mut raw = Vec::new();
                                let mut dec2 = ZlibDecoder::new(data.as_slice());
                                if dec2.read_to_end(&mut raw).is_ok() {
                                    String::from_utf8_lossy(&raw).into_owned()
                                } else {
                                    String::new()
                                }
                            }
                        }
                    } else {
                        String::new()
                    };

                    scripts.push(ScriptEntry { id, title, code });
                }
            }
        }
    } else {
        return Err("Root element is not an array".to_string());
    }

    Ok(scripts)
}

fn save_scripts(value: &mut Value, updated_scripts: Vec<ScriptEntry>) -> Result<(), String> {
    if let Value::Array(ref mut arr) = value {
        for item in arr.iter_mut() {
            if let Value::Array(ref mut script_tuple) = item {
                if script_tuple.len() >= 3 {
                    let id = match &script_tuple[0] {
                        Value::Integer(i) => *i as i64,
                        _ => -1,
                    };

                    if id != -1 {
                        if let Some(updated_script) = updated_scripts.iter().find(|s| s.id == id) {
                            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                            encoder.write_all(updated_script.code.as_bytes()).map_err(|e| e.to_string())?;
                            let compressed_bytes = encoder.finish().map_err(|e| e.to_string())?;

                            match &mut script_tuple[2] {
                                Value::String(s) => {
                                    s.data = compressed_bytes;
                                },
                                Value::Instance(inst) => {
                                    if let Value::String(s) = &mut *inst.value {
                                        s.data = compressed_bytes;
                                    }
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    } else {
        Err("Root element is not an array".to_string())
    }
}

// MapXXX.rvdata / MapXXX.rvdata2 파서
fn parse_map_events(value: Value) -> Result<Vec<ScriptEntry>, String> {
    let mut entries = Vec::new();
    let fields = get_fields(&value)?;

    let events_val = fields.get(&Symbol::from("@events"))
        .ok_or_else(|| "Map @events field not found".to_string())?;

    if let Value::Hash(events_hash) = events_val {
        for (event_id_val, event_val) in events_hash {
            let event_id = match event_id_val {
                Value::Integer(i) => *i,
                _ => continue,
            };

            let event_fields = get_fields(event_val)?;
            let pages_val = event_fields.get(&Symbol::from("@pages"))
                .ok_or_else(|| "Event @pages field not found".to_string())?;

            if let Value::Array(pages) = pages_val {
                for (page_idx, page_val) in pages.iter().enumerate() {
                    let page_fields = get_fields(page_val)?;
                    let list_val = page_fields.get(&Symbol::from("@list"))
                        .ok_or_else(|| "Page @list field not found".to_string())?;

                    if let Value::Array(commands) = list_val {
                        for (cmd_idx, cmd_val) in commands.iter().enumerate() {
                            let cmd_fields = get_fields(cmd_val)?;
                            let code_val = cmd_fields.get(&Symbol::from("@code"))
                                .ok_or_else(|| "Command @code not found".to_string())?;
                            let code = match code_val {
                                Value::Integer(c) => *c,
                                _ => 0,
                            };

                            if code == 401 || code == 101 {
                                let params_val = cmd_fields.get(&Symbol::from("@parameters"))
                                    .ok_or_else(|| "Command @parameters not found".to_string())?;
                                if let Value::Array(params) = params_val {
                                    if !params.is_empty() {
                                        if let Some(text) = get_string_value(&params[0]) {
                                            if has_japanese(&text) {
                                                // ID 인코딩: event_id, page_idx, cmd_idx 조합
                                                let entry_id = (event_id as i64) * 1_000_000_000 + (page_idx as i64) * 1_000_000 + (cmd_idx as i64);
                                                entries.push(ScriptEntry {
                                                    id: entry_id,
                                                    title: format!("Event {} (Page {}) - Line {}", event_id, page_idx + 1, cmd_idx),
                                                    code: text,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

fn save_map_events(value: &mut Value, updated_entries: Vec<ScriptEntry>) -> Result<(), String> {
    let fields = get_fields_mut(value)?;
    let events_val = fields.get_mut(&Symbol::from("@events"))
        .ok_or_else(|| "Map @events field not found".to_string())?;

    if let Value::Hash(events_hash) = events_val {
        for entry in updated_entries {
            let event_id = (entry.id / 1_000_000_000) as i32;
            let page_idx = ((entry.id % 1_000_000_000) / 1_000_000) as i32;
            let cmd_idx = (entry.id % 1_000_000) as i32;

            if let Some(event_val) = events_hash.get_mut(&Value::Integer(event_id)) {
                let event_fields = get_fields_mut(event_val)?;
                let pages_val = event_fields.get_mut(&Symbol::from("@pages"))
                    .ok_or_else(|| "Event @pages field not found".to_string())?;

                if let Value::Array(pages) = pages_val {
                    if let Some(page_val) = pages.get_mut(page_idx as usize) {
                        let page_fields = get_fields_mut(page_val)?;
                        let list_val = page_fields.get_mut(&Symbol::from("@list"))
                            .ok_or_else(|| "Page @list field not found".to_string())?;

                        if let Value::Array(commands) = list_val {
                            if let Some(cmd_val) = commands.get_mut(cmd_idx as usize) {
                                let cmd_fields = get_fields_mut(cmd_val)?;
                                let params_val = cmd_fields.get_mut(&Symbol::from("@parameters"))
                                    .ok_or_else(|| "Command @parameters not found".to_string())?;

                                if let Value::Array(params) = params_val {
                                    if !params.is_empty() {
                                        set_string_value(&mut params[0], entry.code)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// CommonEvents.rvdata / CommonEvents.rvdata2 파서
fn parse_common_events(value: Value) -> Result<Vec<ScriptEntry>, String> {
    let mut entries = Vec::new();

    if let Value::Array(arr) = value {
        for (event_idx, event_val) in arr.iter().enumerate() {
            if event_val.is_nil() {
                continue;
            }
            if let Ok(fields) = get_fields(event_val) {
                let list_val = fields.get(&Symbol::from("@list"));
                if let Some(Value::Array(commands)) = list_val {
                    for (cmd_idx, cmd_val) in commands.iter().enumerate() {
                        if let Ok(cmd_fields) = get_fields(cmd_val) {
                            let code_val = cmd_fields.get(&Symbol::from("@code"));
                            if let Some(Value::Integer(code)) = code_val {
                                if *code == 401 || *code == 101 {
                                    let params_val = cmd_fields.get(&Symbol::from("@parameters"));
                                    if let Some(Value::Array(params)) = params_val {
                                        if !params.is_empty() {
                                            if let Some(text) = get_string_value(&params[0]) {
                                                if has_japanese(&text) {
                                                    let entry_id = (event_idx as i64) * 1_000_000 + (cmd_idx as i64);
                                                    entries.push(ScriptEntry {
                                                        id: entry_id,
                                                        title: format!("CommonEvent {} - Line {}", event_idx, cmd_idx),
                                                        code: text,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

fn save_common_events(value: &mut Value, updated_entries: Vec<ScriptEntry>) -> Result<(), String> {
    if let Value::Array(arr) = value {
        for entry in updated_entries {
            let event_idx = (entry.id / 1_000_000) as usize;
            let cmd_idx = (entry.id % 1_000_000) as usize;

            if let Some(event_val) = arr.get_mut(event_idx as usize) {
                if let Ok(fields) = get_fields_mut(event_val) {
                    let list_val = fields.get_mut(&Symbol::from("@list"));
                    if let Some(Value::Array(commands)) = list_val {
                        if let Some(cmd_val) = commands.get_mut(cmd_idx as usize) {
                            if let Ok(cmd_fields) = get_fields_mut(cmd_val) {
                                let params_val = cmd_fields.get_mut(&Symbol::from("@parameters"));
                                if let Some(Value::Array(params)) = params_val {
                                    if !params.is_empty() {
                                        set_string_value(&mut params[0], entry.code)?;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

// Database (Items, Skills, Actors 등) 파서
fn parse_database_items(value: Value, type_name: &str) -> Result<Vec<ScriptEntry>, String> {
    let mut entries = Vec::new();

    if let Value::Array(arr) = value {
        for (item_idx, item_val) in arr.iter().enumerate() {
            if item_val.is_nil() {
                continue;
            }
            if let Ok(fields) = get_fields(item_val) {
                // @name 필드 번역
                if let Some(name_val) = fields.get(&Symbol::from("@name")) {
                    if let Some(name) = get_string_value(name_val) {
                        if has_japanese(&name) {
                            let entry_id = (item_idx as i64) * 10 + 1;
                            entries.push(ScriptEntry {
                                id: entry_id,
                                title: format!("{} {} [Name]", type_name, item_idx),
                                code: name,
                            });
                        }
                    }
                }

                // @description 또는 @profile 필드 번역
                let desc_symbol = if type_name == "Actor" {
                    Symbol::from("@profile")
                } else {
                    Symbol::from("@description")
                };

                if let Some(desc_val) = fields.get(&desc_symbol) {
                    if let Some(desc) = get_string_value(desc_val) {
                        if has_japanese(&desc) {
                            let entry_id = (item_idx as i64) * 10 + 2;
                            entries.push(ScriptEntry {
                                id: entry_id,
                                title: format!("{} {} [Description]", type_name, item_idx),
                                code: desc,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(entries)
}

fn save_database_items(value: &mut Value, updated_entries: Vec<ScriptEntry>) -> Result<(), String> {
    if let Value::Array(arr) = value {
        for entry in updated_entries {
            let item_idx = (entry.id / 10) as usize;
            let field_type = entry.id % 10; // 1 = name, 2 = description/profile

            if let Some(item_val) = arr.get_mut(item_idx as usize) {
                if let Ok(fields) = get_fields_mut(item_val) {
                    if field_type == 1 {
                        if let Some(name_val) = fields.get_mut(&Symbol::from("@name")) {
                            set_string_value(name_val, entry.code)?;
                        }
                    } else if field_type == 2 {
                        let desc_symbol = Symbol::from("@description");
                        let profile_symbol = Symbol::from("@profile");

                        if fields.contains_key(&desc_symbol) {
                            if let Some(desc_val) = fields.get_mut(&desc_symbol) {
                                set_string_value(desc_val, entry.code)?;
                            }
                        } else if fields.contains_key(&profile_symbol) {
                            if let Some(profile_val) = fields.get_mut(&profile_symbol) {
                                set_string_value(profile_val, entry.code)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
