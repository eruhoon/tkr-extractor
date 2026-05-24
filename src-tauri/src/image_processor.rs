use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct OcrTextRegion {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

use ab_glyph::{FontRef, PxScale};
use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect as ImageRect;

pub fn process_image_text(
    input_path: &str,
    output_path: &str,
    regions: Vec<OcrTextRegion>,
) -> std::result::Result<(), String> {
    let img = image::open(input_path).map_err(|e| format!("Failed to open image: {}", e))?;
    let mut rgba_img = img.to_rgba8();

    let font_data = std::fs::read("C:/Windows/Fonts/malgun.ttf")
        .or_else(|_| std::fs::read("C:/Windows/Fonts/msgothic.ttc"))
        .or_else(|_| std::fs::read("C:/Windows/Fonts/arial.ttf"))
        .map_err(|_| "Failed to find a suitable font".to_string())?;
        
    let font = FontRef::try_from_slice(&font_data).map_err(|e| format!("Error parsing Font: {}", e))?;

    for region in regions {
        // 1. 원본 글자의 진짜(가장 진한) 색상 추출
        let text_color = get_core_text_color(&rgba_img, &region);
        
        let x_start = region.x.max(0.0) as u32;
        let y_start = region.y.max(0.0) as u32;
        let x_end = (region.x + region.w).min(rgba_img.width() as f32) as u32;
        let y_end = (region.y + region.h).min(rgba_img.height() as f32) as u32;
        
        // 2. 배경 훼손 없이 "글자만" 스마트하게 지우기 (크로마키 방식)
        for y in y_start..y_end {
            for x in x_start..x_end {
                let p = rgba_img.get_pixel(x, y);
                // 픽셀이 추출한 텍스트 색상과 비슷하고, 불투명하다면 지운다
                if is_similar_color(p, &text_color) && p[3] > 50 {
                    rgba_img.put_pixel(x, y, Rgba([0, 0, 0, 0])); // 투명하게 지움
                }
            }
        }
        
        // 3. 폰트 크기 계산 (약간 여유있게)
        let scale = PxScale::from(region.h.max(12.0) * 0.85);
        
        // 4. 게임 UI 느낌을 살리는 외곽선(Outline) 효과 그리기
        let outline_color = get_outline_color(&text_color);
        let offsets = [
            (-1, -1), (0, -1), (1, -1),
            (-1, 0),           (1, 0),
            (-1, 1),  (0, 1),  (1, 1)
        ];
        
        for (dx, dy) in offsets.iter() {
            draw_text_mut(
                &mut rgba_img, 
                outline_color, 
                (x_start as i32) + dx, 
                (y_start as i32) + dy, 
                scale, 
                &font, 
                &region.text
            );
        }
        
        // 5. 그 위에 진짜 색상으로 텍스트 덮기
        draw_text_mut(
            &mut rgba_img, 
            text_color, 
            x_start as i32, 
            y_start as i32, 
            scale, 
            &font, 
            &region.text
        );
    }
    
    rgba_img.save(output_path).map_err(|e| format!("Failed to save image: {}", e))?;
    Ok(())
}

// 안티앨리어싱 된 흐린 테두리를 빼고 가장 불투명한 코어 픽셀들의 색상만 가져옴
fn get_core_text_color(img: &RgbaImage, region: &OcrTextRegion) -> Rgba<u8> {
    let mut text_r = 0u64; let mut text_g = 0u64; let mut text_b = 0u64;
    let mut text_count = 0u64;

    let x_start = region.x.max(0.0) as u32;
    let y_start = region.y.max(0.0) as u32;
    let x_end = (region.x + region.w).min(img.width() as f32) as u32;
    let y_end = (region.y + region.h).min(img.height() as f32) as u32;

    for y in y_start..y_end {
        for x in x_start..x_end {
            let p = img.get_pixel(x, y);
            // 알파값이 200 이상인 아주 쨍한(진짜 글자) 픽셀만 평균을 냄
            if p[3] > 200 {
                text_r += p[0] as u64; text_g += p[1] as u64; text_b += p[2] as u64;
                text_count += 1;
            }
        }
    }

    if text_count > 0 {
        Rgba([(text_r / text_count) as u8, (text_g / text_count) as u8, (text_b / text_count) as u8, 255])
    } else {
        // 불투명 픽셀을 못찾았다면 기본 흰색
        Rgba([255, 255, 255, 255])
    }
}

// 색상이 서로 얼마나 비슷한지(유클리드 거리) 판단
fn is_similar_color(p1: &Rgba<u8>, p2: &Rgba<u8>) -> bool {
    let diff_r = (p1[0] as i32 - p2[0] as i32).abs();
    let diff_g = (p1[1] as i32 - p2[1] as i32).abs();
    let diff_b = (p1[2] as i32 - p2[2] as i32).abs();
    
    // 색상 차이가 적당히 가까우면 같은 글자 영역으로 판단
    diff_r < 60 && diff_g < 60 && diff_b < 60
}

// 텍스트 밝기에 따라 외곽선 색상을 까만색 또는 하얀색으로 지정
fn get_outline_color(text_color: &Rgba<u8>) -> Rgba<u8> {
    let lum = 0.299 * text_color[0] as f32 + 0.587 * text_color[1] as f32 + 0.114 * text_color[2] as f32;
    if lum > 100.0 {
        Rgba([0, 0, 0, 255]) // 밝은 글씨 -> 검은 외곽선
    } else {
        Rgba([255, 255, 255, 255]) // 어두운 글씨 -> 하얀 외곽선
    }
}
