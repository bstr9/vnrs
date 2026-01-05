//! Style constants and color definitions for the UI.

use egui::{Color32, FontData, FontDefinitions, FontFamily};

// Trading direction colors
pub const COLOR_LONG: Color32 = Color32::from_rgb(255, 80, 80);     // Red for long
pub const COLOR_SHORT: Color32 = Color32::from_rgb(80, 200, 80);    // Green for short

// Bid/Ask colors
pub const COLOR_BID: Color32 = Color32::from_rgb(255, 174, 201);    // Pink for bid
pub const COLOR_ASK: Color32 = Color32::from_rgb(160, 255, 160);    // Light green for ask

// Status colors
pub const COLOR_POSITIVE: Color32 = Color32::from_rgb(255, 80, 80);
pub const COLOR_NEGATIVE: Color32 = Color32::from_rgb(80, 200, 80);

// Background colors
pub const COLOR_BG_DARK: Color32 = Color32::from_rgb(30, 30, 30);
pub const COLOR_BG_MEDIUM: Color32 = Color32::from_rgb(45, 45, 45);
pub const COLOR_BG_LIGHT: Color32 = Color32::from_rgb(60, 60, 60);

// Text colors
pub const COLOR_TEXT_PRIMARY: Color32 = Color32::from_rgb(220, 220, 220);
pub const COLOR_TEXT_SECONDARY: Color32 = Color32::from_rgb(160, 160, 160);
pub const COLOR_TEXT_DISABLED: Color32 = Color32::from_rgb(100, 100, 100);

/// Setup Chinese font support
pub fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    
    // Try to load system Chinese fonts
    #[cfg(target_os = "windows")]
    {
        // Windows: Microsoft YaHei
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts.font_data.insert(
                "chinese".to_owned(),
                FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("chinese".to_owned());
        } else if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\simsun.ttc") {
            // Fallback to SimSun
            fonts.font_data.insert(
                "chinese".to_owned(),
                FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("chinese".to_owned());
        }
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS: PingFang SC
        if let Ok(font_data) = std::fs::read("/System/Library/Fonts/PingFang.ttc") {
            fonts.font_data.insert(
                "chinese".to_owned(),
                FontData::from_owned(font_data).into(),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "chinese".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("chinese".to_owned());
        }
    }
    
    #[cfg(target_os = "linux")]
    {
        // Linux: Noto Sans CJK or WenQuanYi
        let font_paths = [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
        ];
        for path in font_paths {
            if let Ok(font_data) = std::fs::read(path) {
                fonts.font_data.insert(
                    "chinese".to_owned(),
                    FontData::from_owned(font_data).into(),
                );
                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .insert(0, "chinese".to_owned());
                fonts
                    .families
                    .entry(FontFamily::Monospace)
                    .or_default()
                    .push("chinese".to_owned());
                break;
            }
        }
    }
    
    ctx.set_fonts(fonts);
}

/// Apply dark theme to egui context
pub fn apply_dark_theme(ctx: &egui::Context) {
    // Setup Chinese fonts first
    setup_chinese_fonts(ctx);
    
    let mut style = (*ctx.style()).clone();
    
    // Set dark visuals
    style.visuals = egui::Visuals::dark();
    
    // Customize colors
    style.visuals.window_fill = COLOR_BG_DARK;
    style.visuals.panel_fill = COLOR_BG_MEDIUM;
    style.visuals.faint_bg_color = COLOR_BG_LIGHT;
    
    // Widget colors
    style.visuals.widgets.inactive.bg_fill = COLOR_BG_MEDIUM;
    style.visuals.widgets.hovered.bg_fill = COLOR_BG_LIGHT;
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(80, 80, 80);
    
    // Text colors
    style.visuals.widgets.inactive.fg_stroke.color = COLOR_TEXT_PRIMARY;
    style.visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    
    ctx.set_style(style);
}

/// Get color based on PnL value
pub fn get_pnl_color(value: f64) -> Color32 {
    if value >= 0.0 {
        COLOR_POSITIVE
    } else {
        COLOR_NEGATIVE
    }
}

/// Get direction color
pub fn get_direction_color(is_long: bool) -> Color32 {
    if is_long {
        COLOR_LONG
    } else {
        COLOR_SHORT
    }
}
