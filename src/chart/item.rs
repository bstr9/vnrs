//! Chart items for rendering candlesticks and volume bars.

use egui::{Pos2, Rect, Stroke, Ui};
use super::base::{UP_COLOR, DOWN_COLOR, STAY_COLOR, BAR_WIDTH, BLACK_COLOR};
use super::manager::BarManager;

/// Trait for chart items that can be drawn
pub trait ChartItem {
    /// Get the Y-axis range for the given X-axis range
    fn get_y_range(&self, manager: &BarManager, min_ix: Option<usize>, max_ix: Option<usize>) -> (f64, f64);
    
    /// Get info text for a specific bar index
    fn get_info_text(&self, manager: &BarManager, ix: usize) -> String;
    
    /// Draw the item
    fn draw(
        &self,
        ui: &mut Ui,
        manager: &BarManager,
        rect: Rect,
        min_ix: usize,
        max_ix: usize,
        y_min: f64,
        y_max: f64,
    );
}

/// Candlestick chart item
pub struct CandleItem;

impl Default for CandleItem {
    fn default() -> Self {
        Self::new()
    }
}

impl CandleItem {
    pub fn new() -> Self {
        Self
    }
    
    /// Convert price to screen Y coordinate
    fn price_to_y(&self, price: f64, rect: Rect, y_min: f64, y_max: f64) -> f32 {
        let y_range = y_max - y_min;
        if y_range == 0.0 {
            return rect.center().y;
        }
        let normalized = (price - y_min) / y_range;
        rect.bottom() - (normalized as f32 * rect.height())
    }
    
    /// Convert bar index to screen X coordinate
    fn index_to_x(&self, ix: usize, rect: Rect, min_ix: usize, max_ix: usize) -> f32 {
        let bar_count = (max_ix - min_ix + 1) as f32;
        let bar_width = rect.width() / bar_count;
        rect.left() + (ix - min_ix) as f32 * bar_width + bar_width * 0.5
    }
}

impl ChartItem for CandleItem {
    fn get_y_range(&self, manager: &BarManager, min_ix: Option<usize>, max_ix: Option<usize>) -> (f64, f64) {
        manager.get_price_range(min_ix, max_ix)
    }
    
    fn get_info_text(&self, manager: &BarManager, ix: usize) -> String {
        if let Some(bar) = manager.get_bar(ix as f64) {
            format!(
                "日期\n{}\n\n时间\n{}\n\n开盘\n{:.4}\n\n最高\n{:.4}\n\n最低\n{:.4}\n\n收盘\n{:.4}",
                bar.datetime.format("%Y-%m-%d"),
                bar.datetime.format("%H:%M"),
                bar.open_price,
                bar.high_price,
                bar.low_price,
                bar.close_price
            )
        } else {
            String::new()
        }
    }
    
    fn draw(
        &self,
        ui: &mut Ui,
        manager: &BarManager,
        rect: Rect,
        min_ix: usize,
        max_ix: usize,
        y_min: f64,
        y_max: f64,
    ) {
        let painter = ui.painter();
        let bar_count = (max_ix - min_ix + 1) as f32;
        let bar_pixel_width = rect.width() / bar_count;
        let candle_width = (bar_pixel_width * BAR_WIDTH * 2.0).max(1.0);
        
        for ix in min_ix..=max_ix {
            if let Some(bar) = manager.get_bar(ix as f64) {
                let x = self.index_to_x(ix, rect, min_ix, max_ix);
                
                // Determine color based on price movement
                let (color, fill) = if bar.close_price > bar.open_price {
                    (UP_COLOR, false) // Hollow candle for up
                } else if bar.close_price < bar.open_price {
                    (DOWN_COLOR, true) // Filled candle for down
                } else {
                    (STAY_COLOR, false)
                };
                
                let stroke = Stroke::new(1.0, color);
                
                // Draw high-low line (wick)
                let high_y = self.price_to_y(bar.high_price, rect, y_min, y_max);
                let low_y = self.price_to_y(bar.low_price, rect, y_min, y_max);
                painter.line_segment([Pos2::new(x, high_y), Pos2::new(x, low_y)], stroke);
                
                // Draw candle body
                let open_y = self.price_to_y(bar.open_price, rect, y_min, y_max);
                let close_y = self.price_to_y(bar.close_price, rect, y_min, y_max);
                
                if (open_y - close_y).abs() < 1.0 {
                    // Draw a horizontal line for doji
                    painter.line_segment(
                        [
                            Pos2::new(x - candle_width * 0.5, open_y),
                            Pos2::new(x + candle_width * 0.5, open_y),
                        ],
                        stroke,
                    );
                } else {
                    let body_rect = Rect::from_min_max(
                        Pos2::new(x - candle_width * 0.5, open_y.min(close_y)),
                        Pos2::new(x + candle_width * 0.5, open_y.max(close_y)),
                    );
                    
                    if fill {
                        painter.rect_filled(body_rect, 0.0, color);
                    } else {
                        painter.rect_filled(body_rect, 0.0, BLACK_COLOR);
                        painter.rect_stroke(body_rect, 0.0, stroke, egui::StrokeKind::Inside);
                    }
                }
            }
        }
    }
}

/// Volume bar chart item
pub struct VolumeItem;

impl Default for VolumeItem {
    fn default() -> Self {
        Self::new()
    }
}

impl VolumeItem {
    pub fn new() -> Self {
        Self
    }
    
    /// Convert volume to screen Y coordinate
    fn volume_to_y(&self, volume: f64, rect: Rect, y_max: f64) -> f32 {
        if y_max == 0.0 {
            return rect.bottom();
        }
        let normalized = volume / y_max;
        rect.bottom() - (normalized as f32 * rect.height())
    }
    
    /// Convert bar index to screen X coordinate
    fn index_to_x(&self, ix: usize, rect: Rect, min_ix: usize, max_ix: usize) -> f32 {
        let bar_count = (max_ix - min_ix + 1) as f32;
        let bar_width = rect.width() / bar_count;
        rect.left() + (ix - min_ix) as f32 * bar_width + bar_width * 0.5
    }
}

impl ChartItem for VolumeItem {
    fn get_y_range(&self, manager: &BarManager, min_ix: Option<usize>, max_ix: Option<usize>) -> (f64, f64) {
        manager.get_volume_range(min_ix, max_ix)
    }
    
    fn get_info_text(&self, manager: &BarManager, ix: usize) -> String {
        if let Some(bar) = manager.get_bar(ix as f64) {
            format!("成交量\n{:.2}", bar.volume)
        } else {
            String::new()
        }
    }
    
    fn draw(
        &self,
        ui: &mut Ui,
        manager: &BarManager,
        rect: Rect,
        min_ix: usize,
        max_ix: usize,
        _y_min: f64,
        y_max: f64,
    ) {
        let painter = ui.painter();
        let bar_count = (max_ix - min_ix + 1) as f32;
        let bar_pixel_width = rect.width() / bar_count;
        let volume_bar_width = (bar_pixel_width * BAR_WIDTH * 2.0).max(1.0);
        
        for ix in min_ix..=max_ix {
            if let Some(bar) = manager.get_bar(ix as f64) {
                let x = self.index_to_x(ix, rect, min_ix, max_ix);
                
                // Determine color based on price movement
                let color = if bar.close_price > bar.open_price {
                    UP_COLOR
                } else if bar.close_price < bar.open_price {
                    DOWN_COLOR
                } else {
                    STAY_COLOR
                };
                
                // Draw volume bar
                let top_y = self.volume_to_y(bar.volume, rect, y_max);
                let bottom_y = rect.bottom();
                
                let bar_rect = Rect::from_min_max(
                    Pos2::new(x - volume_bar_width * 0.5, top_y),
                    Pos2::new(x + volume_bar_width * 0.5, bottom_y),
                );
                
                painter.rect_filled(bar_rect, 0.0, color);
            }
        }
    }
}

/// Trade marker for showing buy/sell/short/cover signals
#[derive(Clone)]
pub struct TradeMarker {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub price: f64,
    pub direction: TradeDirection,
    pub volume: f64,
}

/// Trade direction
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TradeDirection {
    Buy,
    Sell,
    Short,
    Cover,
}

/// Trade pair for showing profit/loss lines
#[derive(Clone)]
pub struct TradePair {
    pub open_datetime: chrono::DateTime<chrono::Utc>,
    pub open_price: f64,
    pub close_datetime: chrono::DateTime<chrono::Utc>,
    pub close_price: f64,
    pub direction: TradeDirection,
    pub volume: f64,
    pub is_profit: bool,
}

/// Trade overlay item for showing trades on the chart
pub struct TradeOverlay {
    pub markers: Vec<TradeMarker>,
    pub pairs: Vec<TradePair>,
}

impl Default for TradeOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl TradeOverlay {
    pub fn new() -> Self {
        Self {
            markers: Vec::new(),
            pairs: Vec::new(),
        }
    }
    
    pub fn add_marker(&mut self, marker: TradeMarker) {
        self.markers.push(marker);
    }
    
    pub fn add_pair(&mut self, pair: TradePair) {
        self.pairs.push(pair);
    }
    
    pub fn clear(&mut self) {
        self.markers.clear();
        self.pairs.clear();
    }
    
    /// Draw trade overlay on the candle chart
    pub fn draw(
        &self,
        ui: &mut Ui,
        manager: &BarManager,
        rect: Rect,
        min_ix: usize,
        max_ix: usize,
        y_min: f64,
        y_max: f64,
    ) {
        use super::base::{PROFIT_COLOR, LOSS_COLOR, BUY_COLOR, SHORT_COLOR};
        
        let painter = ui.painter();
        let bar_count = (max_ix - min_ix + 1) as f32;
        let bar_pixel_width = rect.width() / bar_count;
        
        let price_to_y = |price: f64| -> f32 {
            let y_range = y_max - y_min;
            if y_range == 0.0 {
                return rect.center().y;
            }
            let normalized = (price - y_min) / y_range;
            rect.bottom() - (normalized as f32 * rect.height())
        };
        
        let index_to_x = |ix: usize| -> f32 {
            rect.left() + (ix - min_ix) as f32 * bar_pixel_width + bar_pixel_width * 0.5
        };
        
        // Draw trade pair lines
        for pair in &self.pairs {
            if let (Some(open_ix), Some(close_ix)) = (
                manager.get_index(pair.open_datetime),
                manager.get_index(pair.close_datetime),
            ) {
                if open_ix > max_ix || close_ix < min_ix {
                    continue;
                }
                
                let color = if pair.is_profit { PROFIT_COLOR } else { LOSS_COLOR };
                let stroke = Stroke::new(1.0, color);
                
                let open_x = index_to_x(open_ix);
                let open_y = price_to_y(pair.open_price);
                let close_x = index_to_x(close_ix);
                let close_y = price_to_y(pair.close_price);
                
                painter.line_segment([Pos2::new(open_x, open_y), Pos2::new(close_x, close_y)], stroke);
            }
        }
        
        // Draw trade markers (triangles)
        let triangle_size = 8.0;
        for marker in &self.markers {
            if let Some(ix) = manager.get_index(marker.datetime) {
                if ix < min_ix || ix > max_ix {
                    continue;
                }
                
                let x = index_to_x(ix);
                let y = price_to_y(marker.price);
                
                let (color, points) = match marker.direction {
                    TradeDirection::Buy | TradeDirection::Cover => {
                        // Up arrow
                        let color = if marker.direction == TradeDirection::Buy {
                            BUY_COLOR
                        } else {
                            SHORT_COLOR
                        };
                        let points = vec![
                            Pos2::new(x, y - triangle_size),
                            Pos2::new(x - triangle_size * 0.6, y + triangle_size * 0.5),
                            Pos2::new(x + triangle_size * 0.6, y + triangle_size * 0.5),
                        ];
                        (color, points)
                    }
                    TradeDirection::Sell | TradeDirection::Short => {
                        // Down arrow
                        let color = if marker.direction == TradeDirection::Sell {
                            BUY_COLOR
                        } else {
                            SHORT_COLOR
                        };
                        let points = vec![
                            Pos2::new(x, y + triangle_size),
                            Pos2::new(x - triangle_size * 0.6, y - triangle_size * 0.5),
                            Pos2::new(x + triangle_size * 0.6, y - triangle_size * 0.5),
                        ];
                        (color, points)
                    }
                };
                
                painter.add(egui::Shape::convex_polygon(
                    points,
                    color,
                    Stroke::NONE,
                ));
            }
        }
    }
}
