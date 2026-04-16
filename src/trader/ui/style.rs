//! Style constants and color definitions for the UI.

use egui::{Color32, FontData, FontDefinitions, FontFamily};

// Trading direction colors
pub const COLOR_LONG: Color32 = Color32::from_rgb(255, 80, 80); // Red for long
pub const COLOR_SHORT: Color32 = Color32::from_rgb(80, 200, 80); // Green for short

// Bid/Ask colors
pub const COLOR_BID: Color32 = Color32::from_rgb(255, 174, 201); // Pink for bid
pub const COLOR_ASK: Color32 = Color32::from_rgb(160, 255, 160); // Light green for ask

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

// Layout constants
pub const GRID_SPACING: [f32; 2] = [10.0, 5.0]; // Default grid spacing [horizontal, vertical]
pub const TABLE_HEADER_HEIGHT: f32 = 20.0;
pub const TABLE_ROW_HEIGHT: f32 = 18.0;
pub const TABLE_COL_MIN_WIDTH: f32 = 60.0;

/// Setup Chinese font support
pub fn setup_chinese_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Try to load system Chinese fonts
    #[cfg(target_os = "windows")]
    {
        // Windows: Microsoft YaHei
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts
                .font_data
                .insert("chinese".to_owned(), FontData::from_owned(font_data).into());
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
            fonts
                .font_data
                .insert("chinese".to_owned(), FontData::from_owned(font_data).into());
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
            fonts
                .font_data
                .insert("chinese".to_owned(), FontData::from_owned(font_data).into());
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
                fonts
                    .font_data
                    .insert("chinese".to_owned(), FontData::from_owned(font_data).into());
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

/// Apply light theme to egui context
pub fn apply_light_theme(ctx: &egui::Context) {
    // Setup Chinese fonts first
    setup_chinese_fonts(ctx);

    let mut style = (*ctx.style()).clone();

    // Set light visuals
    style.visuals = egui::Visuals::light();

    // Customize colors for light theme
    style.visuals.window_fill = Color32::from_rgb(245, 245, 245);
    style.visuals.panel_fill = Color32::from_rgb(235, 235, 235);
    style.visuals.faint_bg_color = Color32::from_rgb(220, 220, 220);

    // Widget colors
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(240, 240, 240);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(225, 225, 225);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(210, 210, 210);

    // Text colors
    style.visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(30, 30, 30);
    style.visuals.widgets.hovered.fg_stroke.color = Color32::BLACK;
    style.visuals.widgets.active.fg_stroke.color = Color32::BLACK;

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

// ============================================================================
// Toast notification system
// ============================================================================

/// Toast type determines the border color
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastType {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastType {
    fn border_color(&self) -> Color32 {
        match self {
            ToastType::Info => Color32::from_rgb(80, 150, 255), // Blue
            ToastType::Success => Color32::from_rgb(80, 200, 80), // Green
            ToastType::Warning => Color32::from_rgb(255, 200, 50), // Yellow
            ToastType::Error => Color32::from_rgb(255, 80, 80), // Red
        }
    }
}

/// A single toast notification
pub struct Toast {
    pub message: String,
    pub toast_type: ToastType,
    pub created_at: std::time::Instant,
    pub duration: std::time::Duration,
}

impl Toast {
    pub fn new(message: impl Into<String>, toast_type: ToastType) -> Self {
        Self {
            message: message.into(),
            toast_type,
            created_at: std::time::Instant::now(),
            duration: std::time::Duration::from_secs(3),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.duration
    }
}

/// Manager for toast notifications
pub struct ToastManager {
    pub toasts: Vec<Toast>,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastManager {
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    /// Add a new toast notification
    pub fn add(&mut self, message: &str, toast_type: ToastType) {
        self.toasts.push(Toast::new(message, toast_type));
    }

    /// Show all active toasts and remove expired ones
    pub fn show(&mut self, ctx: &egui::Context) {
        // Remove expired toasts
        self.toasts.retain(|t| !t.is_expired());

        if self.toasts.is_empty() {
            return;
        }

        #[allow(deprecated)]
        let screen = ctx.screen_rect();
        let margin = 16.0;
        let toast_height = 28.0;
        let toast_padding_x = 12.0;
        let toast_spacing = 6.0;

        for (i, toast) in self.toasts.iter().enumerate() {
            let border_color = toast.toast_type.border_color();

            // Calculate alpha for fade-out in the last 500ms
            let elapsed = toast.created_at.elapsed().as_millis() as u64;
            let duration_ms = toast.duration.as_millis() as u64;
            let remaining = duration_ms.saturating_sub(elapsed);
            let alpha = if remaining < 500 {
                (remaining as f32 / 500.0 * 200.0) as u8
            } else {
                200
            };

            // Calculate toast position (bottom-right, stacking upward)
            let y = screen.bottom() - margin - (i as f32 + 1.0) * (toast_height + toast_spacing);

            // Measure text width
            let font_id = egui::FontId::proportional(13.0);
            let galley = ctx.fonts_mut(|f| {
                f.layout_no_wrap(toast.message.clone(), font_id.clone(), Color32::WHITE)
            });

            let text_width = galley.size().x;
            let toast_width = text_width + toast_padding_x * 2.0 + 8.0; // 8px for left border

            let toast_rect = egui::Rect::from_min_size(
                egui::Pos2::new(screen.right() - margin - toast_width, y),
                egui::Vec2::new(toast_width, toast_height),
            );

            let area = egui::Area::new(egui::Id::new(format!(
                "toast_{}_{}",
                i,
                toast.created_at.elapsed().as_millis()
            )))
            .pivot(egui::Align2::RIGHT_BOTTOM)
            .fixed_pos(toast_rect.right_bottom())
            .order(egui::Order::Foreground)
            .interactable(false);

            area.show(ctx, |ui| {
                let painter = ui.painter();

                // Background
                painter.rect_filled(
                    toast_rect,
                    4.0,
                    Color32::from_rgba_unmultiplied(40, 40, 40, alpha),
                );

                // Left border (colored by type)
                let border_rect =
                    egui::Rect::from_min_size(toast_rect.min, egui::Vec2::new(4.0, toast_height));
                let border_alpha = (alpha as f32 / 200.0 * 255.0) as u8;
                painter.rect_filled(
                    border_rect,
                    2.0,
                    Color32::from_rgba_unmultiplied(
                        border_color.r(),
                        border_color.g(),
                        border_color.b(),
                        border_alpha,
                    ),
                );

                // Text
                painter.text(
                    egui::Pos2::new(
                        toast_rect.left() + toast_padding_x + 4.0,
                        toast_rect.center().y,
                    ),
                    egui::Align2::LEFT_CENTER,
                    &toast.message,
                    font_id,
                    Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
                );
            });
        }
    }
}
