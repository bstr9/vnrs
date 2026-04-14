//! Base constants and utility functions for the chart module.

use egui::Color32;

// Chart colors
pub const WHITE_COLOR: Color32 = Color32::from_rgb(255, 255, 255);
pub const BLACK_COLOR: Color32 = Color32::from_rgb(0, 0, 0);
pub const GREY_COLOR: Color32 = Color32::from_rgb(100, 100, 100);

// Price movement colors (Chinese style: red up, green down)
pub const UP_COLOR: Color32 = Color32::from_rgb(255, 75, 75);      // Red for price up
pub const DOWN_COLOR: Color32 = Color32::from_rgb(0, 255, 255);    // Cyan for price down
pub const STAY_COLOR: Color32 = Color32::from_rgb(255, 255, 255);  // White for no change

// Cursor color
pub const CURSOR_COLOR: Color32 = Color32::from_rgb(255, 245, 162);

// Profit/Loss colors
pub const PROFIT_COLOR: Color32 = Color32::from_rgb(255, 0, 0);    // Red for profit
pub const LOSS_COLOR: Color32 = Color32::from_rgb(0, 255, 0);      // Green for loss

// Trade marker colors
pub const BUY_COLOR: Color32 = Color32::from_rgb(255, 255, 0);     // Yellow for buy
pub const SELL_COLOR: Color32 = Color32::from_rgb(255, 255, 0);    // Yellow for sell
pub const SHORT_COLOR: Color32 = Color32::from_rgb(255, 0, 255);   // Magenta for short
pub const COVER_COLOR: Color32 = Color32::from_rgb(255, 0, 255);   // Magenta for cover

// Chart dimensions
pub const BAR_WIDTH: f32 = 0.3;
pub const PEN_WIDTH: f32 = 1.0;
pub const AXIS_WIDTH: f32 = 0.8;
pub const MIN_BAR_COUNT: usize = 50;

// Layout constants
pub const MARGIN: f32 = 5.0;
pub const AXIS_X_HEIGHT: f32 = 32.0;
pub const AXIS_Y_WIDTH: f32 = 80.0;
pub const INFO_BOX_WIDTH: f32 = 100.0;
pub const INFO_BOX_HEIGHT: f32 = 320.0;

/// Convert a float value to integer with rounding
#[inline]
pub fn to_int(value: f64) -> i64 {
    value.round() as i64
}

/// Format price with appropriate precision
pub fn format_price(price: f64, decimals: usize) -> String {
    format!("{:.prec$}", price, prec = decimals)
}

/// Format volume with appropriate units (K, M, B)
pub fn format_volume(volume: f64) -> String {
    if volume >= 1_000_000_000.0 {
        format!("{:.2}B", volume / 1_000_000_000.0)
    } else if volume >= 1_000_000.0 {
        format!("{:.2}M", volume / 1_000_000.0)
    } else if volume >= 1_000.0 {
        format!("{:.2}K", volume / 1_000.0)
    } else {
        format!("{:.2}", volume)
    }
}

/// Calculate nice axis tick values
pub fn calculate_axis_ticks(min_val: f64, max_val: f64, max_ticks: usize) -> Vec<f64> {
    if min_val >= max_val {
        return vec![min_val];
    }
    
    let range = max_val - min_val;
    let rough_step = range / max_ticks as f64;
    
    // Find the magnitude of the step
    let magnitude = 10.0_f64.powf(rough_step.log10().floor());
    let residual = rough_step / magnitude;
    
    // Choose a nice step value
    let nice_step = if residual <= 1.5 {
        magnitude
    } else if residual <= 3.0 {
        2.0 * magnitude
    } else if residual <= 7.0 {
        5.0 * magnitude
    } else {
        10.0 * magnitude
    };
    
    // Generate tick values
    let mut ticks = Vec::new();
    let start = (min_val / nice_step).ceil() * nice_step;
    let mut value = start;
    
    while value <= max_val {
        ticks.push(value);
        value += nice_step;
    }
    
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_to_int() {
        assert_eq!(to_int(1.4), 1);
        assert_eq!(to_int(1.5), 2);
        assert_eq!(to_int(1.6), 2);
        assert_eq!(to_int(-1.5), -2);
    }
    
    #[test]
    fn test_format_volume() {
        assert_eq!(format_volume(100.0), "100.00");
        assert_eq!(format_volume(1500.0), "1.50K");
        assert_eq!(format_volume(1500000.0), "1.50M");
        assert_eq!(format_volume(1500000000.0), "1.50B");
    }
    
    #[test]
    fn test_calculate_axis_ticks() {
        let ticks = calculate_axis_ticks(0.0, 100.0, 5);
        assert!(!ticks.is_empty());
        for tick in &ticks {
            assert!(*tick >= 0.0 && *tick <= 100.0);
        }
    }
}
