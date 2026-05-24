use alox_48::Value;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{Read, Write};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct ScriptEntry {
    pub id: i32,
    pub title: String,
    pub code: String,
}

pub fn parse_rvdata(path: &str) -> Result<Vec<ScriptEntry>, String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let value: Value = alox_48::from_bytes(&buffer).map_err(|e| format!("Marshal Error: {:?}", e))?;

    let mut scripts = Vec::new();

    if let Value::Array(arr) = value {
        for item in arr.into_iter() {
            if let Value::Array(script_tuple) = item {
                if script_tuple.len() >= 3 {
                    let id = match &script_tuple[0] {
                        Value::Integer(i) => *i,
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

pub fn save_rvdata(original_path: &str, new_path: &str, updated_scripts: Vec<ScriptEntry>) -> Result<(), String> {
    let mut file = File::open(original_path).map_err(|e| e.to_string())?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

    let mut value: Value = alox_48::from_bytes(&buffer).map_err(|e| format!("Marshal Error: {:?}", e))?;

    if let Value::Array(ref mut arr) = value {
        for item in arr.iter_mut() {
            if let Value::Array(ref mut script_tuple) = item {
                if script_tuple.len() >= 3 {
                    let id = match &script_tuple[0] {
                        Value::Integer(i) => *i,
                        _ => -1,
                    };

                    if id != -1 {
                        if let Some(updated_script) = updated_scripts.iter().find(|s| s.id == id) {
                            // Compress new code
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
    } else {
        return Err("Root element is not an array".to_string());
    }

    let out_bytes = alox_48::to_bytes(&value).map_err(|e| format!("Marshal Error: {:?}", e))?;
    let mut out_file = File::create(new_path).map_err(|e| e.to_string())?;
    out_file.write_all(&out_bytes).map_err(|e| e.to_string())?;

    Ok(())
}
