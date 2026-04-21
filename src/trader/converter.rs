//! Offset converter for handling position offset in different exchanges.

use std::collections::HashMap;

use super::constant::{Direction, Exchange, Offset};
use super::object::{ContractData, OrderData, OrderRequest, PositionData, TradeData};
use tracing::warn;

/// Position holding for tracking long/short positions and frozen amounts
#[derive(Debug, Clone)]
pub struct PositionHolding {
    pub vt_symbol: String,
    pub exchange: Exchange,

    pub active_orders: HashMap<String, OrderData>,

    pub long_pos: f64,
    pub long_yd: f64,
    pub long_td: f64,

    pub short_pos: f64,
    pub short_yd: f64,
    pub short_td: f64,

    pub long_pos_frozen: f64,
    pub long_yd_frozen: f64,
    pub long_td_frozen: f64,

    pub short_pos_frozen: f64,
    pub short_yd_frozen: f64,
    pub short_td_frozen: f64,
}

impl PositionHolding {
    /// Create a new PositionHolding from contract data
    pub fn new(contract: &ContractData) -> Self {
        Self {
            vt_symbol: contract.vt_symbol(),
            exchange: contract.exchange,
            active_orders: HashMap::new(),
            long_pos: 0.0,
            long_yd: 0.0,
            long_td: 0.0,
            short_pos: 0.0,
            short_yd: 0.0,
            short_td: 0.0,
            long_pos_frozen: 0.0,
            long_yd_frozen: 0.0,
            long_td_frozen: 0.0,
            short_pos_frozen: 0.0,
            short_yd_frozen: 0.0,
            short_td_frozen: 0.0,
        }
    }

    /// Update position data
    pub fn update_position(&mut self, position: &PositionData) {
        match position.direction {
            Direction::Long => {
                self.long_pos = position.volume;
                self.long_yd = position.yd_volume;
                self.long_td = self.long_pos - self.long_yd;
            }
            Direction::Short => {
                self.short_pos = position.volume;
                self.short_yd = position.yd_volume;
                self.short_td = self.short_pos - self.short_yd;
            }
            Direction::Net => {
                let pos_change = position.volume - self.long_pos + self.short_pos;
                if pos_change >= 0.0 {
                    self.long_pos += pos_change;
                    self.long_yd += pos_change;
                    self.long_td = self.long_pos - self.long_yd;
                } else {
                    self.short_pos += pos_change.abs();
                    self.short_yd += pos_change.abs();
                    self.short_td = self.short_pos - self.short_yd;
                }
            }
        }
    }

    /// Update order data
    pub fn update_order(&mut self, order: &OrderData) {
        if order.is_active() {
            self.active_orders.insert(order.vt_orderid(), order.clone());
        } else {
            self.active_orders.remove(&order.vt_orderid());
        }

        self.calculate_frozen();
    }

    /// Update order request
    pub fn update_order_request(&mut self, req: &OrderRequest, vt_orderid: &str) {
        let parts: Vec<&str> = vt_orderid.split('.').collect();
        if parts.len() != 2 {
            return;
        }

        let gateway_name = parts[0];
        let orderid = parts[1];

        let order = req.create_order_data(orderid.to_string(), gateway_name.to_string());
        self.update_order(&order);
    }

    /// Update trade data
    pub fn update_trade(&mut self, trade: &TradeData) {
        let direction = match trade.direction {
            Some(d) => d,
            None => return,
        };

        if direction == Direction::Long {
            match trade.offset {
                Offset::Open => {
                    self.long_td += trade.volume;
                }
                Offset::CloseToday => {
                    self.short_td -= trade.volume;
                }
                Offset::CloseYesterday => {
                    self.short_yd -= trade.volume;
                }
                Offset::Close => {
                    if matches!(trade.exchange, Exchange::Shfe | Exchange::Ine) {
                        self.short_yd -= trade.volume;
                    } else {
                        self.short_td -= trade.volume;
                        if self.short_td < 0.0 {
                            self.short_yd += self.short_td;
                            self.short_td = 0.0;
                        }
                    }
                }
                _ => {
                    warn!(
                        "Unexpected offset {:?} in long position update for trade, ignoring",
                        trade.offset
                    );
                }
            }
        } else {
            match trade.offset {
                Offset::Open => {
                    self.short_td += trade.volume;
                }
                Offset::CloseToday => {
                    self.long_td -= trade.volume;
                }
                Offset::CloseYesterday => {
                    self.long_yd -= trade.volume;
                }
                Offset::Close => {
                    if matches!(trade.exchange, Exchange::Shfe | Exchange::Ine) {
                        self.long_yd -= trade.volume;
                    } else {
                        self.long_td -= trade.volume;
                        if self.long_td < 0.0 {
                            self.long_yd += self.long_td;
                            self.long_td = 0.0;
                        }
                    }
                }
                _ => {
                    warn!(
                        "Unexpected offset {:?} in short position update for trade, ignoring",
                        trade.offset
                    );
                }
            }
        }

        self.long_pos = self.long_td + self.long_yd;
        self.short_pos = self.short_td + self.short_yd;

        // Update frozen volume to ensure no more than total volume
        self.sum_pos_frozen();
    }

    /// Calculate frozen positions
    fn calculate_frozen(&mut self) {
        self.long_pos_frozen = 0.0;
        self.long_yd_frozen = 0.0;
        self.long_td_frozen = 0.0;

        self.short_pos_frozen = 0.0;
        self.short_yd_frozen = 0.0;
        self.short_td_frozen = 0.0;

        for order in self.active_orders.values() {
            // Ignore position open orders
            if order.offset == Offset::Open {
                continue;
            }

            let frozen = order.volume - order.traded;
            let direction = match order.direction {
                Some(d) => d,
                None => continue,
            };

            if direction == Direction::Long {
                match order.offset {
                    Offset::CloseToday => {
                        self.short_td_frozen += frozen;
                    }
                    Offset::CloseYesterday => {
                        self.short_yd_frozen += frozen;
                    }
                    Offset::Close => {
                        self.short_td_frozen += frozen;
                        if self.short_td_frozen > self.short_td {
                            self.short_yd_frozen += self.short_td_frozen - self.short_td;
                            self.short_td_frozen = self.short_td;
                        }
                    }
                    Offset::None | Offset::Open => {}
                }
            } else if direction == Direction::Short {
                match order.offset {
                    Offset::CloseToday => {
                        self.long_td_frozen += frozen;
                    }
                    Offset::CloseYesterday => {
                        self.long_yd_frozen += frozen;
                    }
                    Offset::Close => {
                        self.long_td_frozen += frozen;
                        if self.long_td_frozen > self.long_td {
                            self.long_yd_frozen += self.long_td_frozen - self.long_td;
                            self.long_td_frozen = self.long_td;
                        }
                    }
                    Offset::None | Offset::Open => {}
                }
            }
        }

        self.sum_pos_frozen();
    }

    /// Sum position frozen amounts
    fn sum_pos_frozen(&mut self) {
        // Frozen volume should be no more than total volume
        self.long_td_frozen = self.long_td_frozen.min(self.long_td);
        self.long_yd_frozen = self.long_yd_frozen.min(self.long_yd);

        self.short_td_frozen = self.short_td_frozen.min(self.short_td);
        self.short_yd_frozen = self.short_yd_frozen.min(self.short_yd);

        self.long_pos_frozen = self.long_td_frozen + self.long_yd_frozen;
        self.short_pos_frozen = self.short_td_frozen + self.short_yd_frozen;
    }

    /// Convert order request for SHFE exchange
    pub fn convert_order_request_shfe(&self, req: &OrderRequest) -> Vec<OrderRequest> {
        if req.offset == Offset::Open {
            return vec![req.clone()];
        }

        let (pos_available, td_available) = if req.direction == Direction::Long {
            (
                self.short_pos - self.short_pos_frozen,
                self.short_td - self.short_td_frozen,
            )
        } else {
            (
                self.long_pos - self.long_pos_frozen,
                self.long_td - self.long_td_frozen,
            )
        };

        if req.volume > pos_available {
            vec![]
        } else if req.volume <= td_available {
            let mut req_td = req.clone();
            req_td.offset = Offset::CloseToday;
            vec![req_td]
        } else {
            let mut req_list = vec![];

            if td_available > 0.0 {
                let mut req_td = req.clone();
                req_td.offset = Offset::CloseToday;
                req_td.volume = td_available;
                req_list.push(req_td);
            }

            let mut req_yd = req.clone();
            req_yd.offset = Offset::CloseYesterday;
            req_yd.volume = req.volume - td_available;
            req_list.push(req_yd);

            req_list
        }
    }

    /// Convert order request with lock mode
    pub fn convert_order_request_lock(&self, req: &OrderRequest) -> Vec<OrderRequest> {
        let (td_volume, yd_available) = if req.direction == Direction::Long {
            (self.short_td, self.short_yd - self.short_yd_frozen)
        } else {
            (self.long_td, self.long_yd - self.long_yd_frozen)
        };

        let close_yd_exchanges = [Exchange::Shfe, Exchange::Ine];

        // If there is td_volume, we can only lock position
        if td_volume > 0.0 && !close_yd_exchanges.contains(&self.exchange) {
            let mut req_open = req.clone();
            req_open.offset = Offset::Open;
            vec![req_open]
        }
        // If no td_volume, we close opposite yd position first then open new position
        else {
            let close_volume = req.volume.min(yd_available);
            let open_volume = (req.volume - yd_available).max(0.0);
            let mut req_list = vec![];

            if yd_available > 0.0 {
                let mut req_yd = req.clone();
                if close_yd_exchanges.contains(&self.exchange) {
                    req_yd.offset = Offset::CloseYesterday;
                } else {
                    req_yd.offset = Offset::Close;
                }
                req_yd.volume = close_volume;
                req_list.push(req_yd);
            }

            if open_volume > 0.0 {
                let mut req_open = req.clone();
                req_open.offset = Offset::Open;
                req_open.volume = open_volume;
                req_list.push(req_open);
            }

            req_list
        }
    }

    /// Convert order request with net mode
    pub fn convert_order_request_net(&self, req: &OrderRequest) -> Vec<OrderRequest> {
        let (pos_available, td_available, yd_available) = if req.direction == Direction::Long {
            (
                self.short_pos - self.short_pos_frozen,
                self.short_td - self.short_td_frozen,
                self.short_yd - self.short_yd_frozen,
            )
        } else {
            (
                self.long_pos - self.long_pos_frozen,
                self.long_td - self.long_td_frozen,
                self.long_yd - self.long_yd_frozen,
            )
        };

        // Split close order to close today/yesterday for SHFE/INE exchange
        if matches!(req.exchange, Exchange::Shfe | Exchange::Ine) {
            let mut reqs = vec![];
            let mut volume_left = req.volume;

            if td_available > 0.0 {
                let td_volume = td_available.min(volume_left);
                volume_left -= td_volume;

                let mut td_req = req.clone();
                td_req.offset = Offset::CloseToday;
                td_req.volume = td_volume;
                reqs.push(td_req);
            }

            if volume_left > 0.0 && yd_available > 0.0 {
                let yd_volume = yd_available.min(volume_left);
                volume_left -= yd_volume;

                let mut yd_req = req.clone();
                yd_req.offset = Offset::CloseYesterday;
                yd_req.volume = yd_volume;
                reqs.push(yd_req);
            }

            if volume_left > 0.0 {
                let mut open_req = req.clone();
                open_req.offset = Offset::Open;
                open_req.volume = volume_left;
                reqs.push(open_req);
            }

            reqs
        }
        // Just use close for other exchanges
        else {
            let mut reqs = vec![];
            let mut volume_left = req.volume;

            if pos_available > 0.0 {
                let close_volume = pos_available.min(volume_left);
                volume_left -= pos_available;

                let mut close_req = req.clone();
                close_req.offset = Offset::Close;
                close_req.volume = close_volume;
                reqs.push(close_req);
            }

            if volume_left > 0.0 {
                let mut open_req = req.clone();
                open_req.offset = Offset::Open;
                open_req.volume = volume_left;
                reqs.push(open_req);
            }

            reqs
        }
    }
}

/// Type-erased contract lookup function for use in MainEngine
type ContractLookup = Box<dyn Fn(&str) -> Option<ContractData> + Send + Sync>;

/// Offset converter for managing position holdings and order conversion
pub struct OffsetConverter {
    holdings: HashMap<String, PositionHolding>,
    get_contract: ContractLookup,
}

impl OffsetConverter {
    /// Create a new OffsetConverter with a boxed contract lookup function
    pub fn new(get_contract: ContractLookup) -> Self {
        Self {
            holdings: HashMap::new(),
            get_contract,
        }
    }

    /// Update position data
    pub fn update_position(&mut self, position: &PositionData) {
        let vt_symbol = position.vt_symbol();
        if !self.is_convert_required(&vt_symbol) {
            return;
        }

        if let Some(holding) = self.get_position_holding(&vt_symbol) {
            holding.update_position(position);
        }
    }

    /// Update trade data
    pub fn update_trade(&mut self, trade: &TradeData) {
        let vt_symbol = trade.vt_symbol();
        if !self.is_convert_required(&vt_symbol) {
            return;
        }

        if let Some(holding) = self.get_position_holding(&vt_symbol) {
            holding.update_trade(trade);
        }
    }

    /// Update order data
    pub fn update_order(&mut self, order: &OrderData) {
        let vt_symbol = order.vt_symbol();
        if !self.is_convert_required(&vt_symbol) {
            return;
        }

        if let Some(holding) = self.get_position_holding(&vt_symbol) {
            holding.update_order(order);
        }
    }

    /// Update order request
    pub fn update_order_request(&mut self, req: &OrderRequest, vt_orderid: &str) {
        let vt_symbol = req.vt_symbol();
        if !self.is_convert_required(&vt_symbol) {
            return;
        }

        if let Some(holding) = self.get_position_holding(&vt_symbol) {
            holding.update_order_request(req, vt_orderid);
        }
    }

    /// Get position holding for a symbol
    fn get_position_holding(&mut self, vt_symbol: &str) -> Option<&mut PositionHolding> {
        if !self.holdings.contains_key(vt_symbol) {
            if let Some(contract) = (self.get_contract)(vt_symbol) {
                let holding = PositionHolding::new(&contract);
                self.holdings.insert(vt_symbol.to_string(), holding);
            }
        }

        self.holdings.get_mut(vt_symbol)
    }

    /// Convert order request according to given mode
    pub fn convert_order_request(
        &mut self,
        req: &OrderRequest,
        lock: bool,
        net: bool,
    ) -> Vec<OrderRequest> {
        let vt_symbol = req.vt_symbol();
        if !self.is_convert_required(&vt_symbol) {
            return vec![req.clone()];
        }

        let holding = match self.get_position_holding(&vt_symbol) {
            Some(h) => h,
            None => return vec![req.clone()],
        };

        if lock {
            holding.convert_order_request_lock(req)
        } else if net {
            holding.convert_order_request_net(req)
        } else if matches!(req.exchange, Exchange::Shfe | Exchange::Ine) {
            holding.convert_order_request_shfe(req)
        } else {
            vec![req.clone()]
        }
    }

    /// Check if the contract needs offset convert
    pub fn is_convert_required(&self, vt_symbol: &str) -> bool {
        // Only contracts with long-short position mode require convert
        if let Some(contract) = (self.get_contract)(vt_symbol) {
            !contract.net_position
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::constant::{OrderType, Product, Status};

    #[test]
    fn test_position_holding_new() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            Product::Futures,
            300.0,
            0.2,
        );

        let holding = PositionHolding::new(&contract);
        assert_eq!(holding.vt_symbol, "IF2312.CFFEX");
        assert_eq!(holding.long_pos, 0.0);
        assert_eq!(holding.short_pos, 0.0);
    }

    #[test]
    fn test_position_holding_update_position() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            Product::Futures,
            300.0,
            0.2,
        );

        let mut holding = PositionHolding::new(&contract);

        let mut position = PositionData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            Direction::Long,
        );
        position.volume = 10.0;
        position.yd_volume = 5.0;

        holding.update_position(&position);

        assert_eq!(holding.long_pos, 10.0);
        assert_eq!(holding.long_yd, 5.0);
        assert_eq!(holding.long_td, 5.0);
    }

    #[test]
    fn test_offset_converter_new() {
        let converter = OffsetConverter::new(Box::new(|_vt_symbol| None));
        assert!(converter.holdings.is_empty());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_new_zero_frozen() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            Product::Futures,
            300.0,
            0.2,
        );

        let holding = PositionHolding::new(&contract);
        // All frozen amounts should be zero
        assert!((holding.long_pos_frozen - 0.0).abs() < 0.01);
        assert!((holding.long_yd_frozen - 0.0).abs() < 0.01);
        assert!((holding.long_td_frozen - 0.0).abs() < 0.01);
        assert!((holding.short_pos_frozen - 0.0).abs() < 0.01);
        assert!((holding.short_yd_frozen - 0.0).abs() < 0.01);
        assert!((holding.short_td_frozen - 0.0).abs() < 0.01);
        assert!(holding.active_orders.is_empty());
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_update_position_short() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            Product::Futures,
            300.0,
            0.2,
        );

        let mut holding = PositionHolding::new(&contract);

        let mut position = PositionData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            Direction::Short,
        );
        position.volume = 15.0;
        position.yd_volume = 8.0;

        holding.update_position(&position);

        assert!((holding.short_pos - 15.0).abs() < 0.01);
        assert!((holding.short_yd - 8.0).abs() < 0.01);
        assert!((holding.short_td - 7.0).abs() < 0.01); // 15 - 8
        // Long position should remain unchanged
        assert!((holding.long_pos - 0.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_update_position_net_positive() {
        let contract = ContractData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "BTCUSDT".to_string(),
            Product::Spot,
            1.0,
            0.01,
        );

        let mut holding = PositionHolding::new(&contract);

        let mut position = PositionData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Net,
        );
        position.volume = 10.0;

        holding.update_position(&position);

        // Net positive should increase long position
        assert!((holding.long_pos - 10.0).abs() < 0.01);
        assert!((holding.short_pos - 0.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_update_position_net_negative() {
        let contract = ContractData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "BTCUSDT".to_string(),
            Product::Spot,
            1.0,
            0.01,
        );

        let mut holding = PositionHolding::new(&contract);

        // First set a long position
        let mut position1 = PositionData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
        );
        position1.volume = 10.0;
        holding.update_position(&position1);

        // Now update with net negative
        let mut position2 = PositionData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Net,
        );
        position2.volume = 5.0; // Net change = 5 - 10 = -5
        holding.update_position(&position2);

        // Should have short position of 5
        assert!((holding.short_pos - 5.0).abs() < 0.01);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_frozen_with_close_order() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            Product::Futures,
            300.0,
            0.2,
        );

        let mut holding = PositionHolding::new(&contract);

        // Set up a long position
        let mut position = PositionData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            Direction::Long,
        );
        position.volume = 10.0;
        position.yd_volume = 5.0;
        holding.update_position(&position);

        // Add a close order (Short direction with Close offset)
        let order = OrderData {
                    gateway_name: "test".to_string(),
                    symbol: "IF2312".to_string(),
                    exchange: Exchange::Cffex,
                    orderid: "ORDER_1".to_string(),
                    order_type: OrderType::Limit,
                    direction: Some(Direction::Short),
                    offset: Offset::Close,
                    price: 4000.0,
                    volume: 3.0,
                    traded: 0.0,
                    status: Status::NotTraded,
                    datetime: None,
                    reference: String::new(),
                    post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
                };

        holding.update_order(&order);

        // Frozen amount should reflect the close order
        assert!((holding.long_td_frozen - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_offset_converter_no_contract() {
        let mut converter = OffsetConverter::new(Box::new(|_vt_symbol| None));

        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );

        let result = converter.convert_order_request(&req, false, false);
        // Should return original request when contract not found
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].symbol, "BTCUSDT");
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_offset_converter_spot_exchange() {
        let contract = ContractData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            "BTCUSDT".to_string(),
            Product::Spot,
            1.0,
            0.01,
        );

        let mut converter = OffsetConverter::new(Box::new(move |_vt_symbol| {
            Some(contract.clone())
        }));

        let req = OrderRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Direction::Long,
            OrderType::Limit,
            1.0,
        );

        let result = converter.convert_order_request(&req, false, false);
        // Spot trading should return original request
        assert_eq!(result.len(), 1);
        // Spot uses Offset::None (default)
        assert_eq!(result[0].offset, Offset::None);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_convert_order_request_shfe_close() {
        let contract = ContractData::new(
            "test".to_string(),
            "au2312".to_string(),
            Exchange::Shfe,
            "黄金期货".to_string(),
            Product::Futures,
            1000.0,
            0.01,
        );

        let mut holding = PositionHolding::new(&contract);

        // Set up a short position (to close with Long direction)
        let mut position = PositionData::new(
            "test".to_string(),
            "au2312".to_string(),
            Exchange::Shfe,
            Direction::Short,
        );
        position.volume = 10.0;
        position.yd_volume = 4.0; // 6 today, 4 yesterday
        holding.update_position(&position);

        // Close order - should split into CloseToday and CloseYesterday
        let req = OrderRequest {
                    symbol: "au2312".to_string(),
                    exchange: Exchange::Shfe,
                    direction: Direction::Long,
                    order_type: OrderType::Limit,
                    volume: 10.0,
                    price: 400.0,
                    offset: Offset::Close,
                    reference: String::new(),
                    post_only: false,
                    reduce_only: false,
                    expire_time: None,
                    gateway_name: String::new(),
                };

        let result = holding.convert_order_request_shfe(&req);

        // Should split into CloseToday (6) and CloseYesterday (4)
        assert_eq!(result.len(), 2);
        
        let has_close_today = result.iter().any(|r| r.offset == Offset::CloseToday);
        let has_close_yesterday = result.iter().any(|r| r.offset == Offset::CloseYesterday);
        assert!(has_close_today);
        assert!(has_close_yesterday);
    }

    #[test]
    #[allow(clippy::unwrap_used)]
    fn test_position_holding_convert_order_request_net_mode() {
        let contract = ContractData::new(
            "test".to_string(),
            "au2312".to_string(),
            Exchange::Shfe,
            "黄金期货".to_string(),
            Product::Futures,
            1000.0,
            0.01,
        );

        let mut holding = PositionHolding::new(&contract);

        // Set up a short position
        let mut position = PositionData::new(
            "test".to_string(),
            "au2312".to_string(),
            Exchange::Shfe,
            Direction::Short,
        );
        position.volume = 5.0;
        position.yd_volume = 2.0; // 3 today, 2 yesterday
        holding.update_position(&position);

        // Net mode close order that exceeds position
        let req = OrderRequest {
            symbol: "au2312".to_string(),
            exchange: Exchange::Shfe,
            direction: Direction::Long,
            order_type: OrderType::Limit,
            volume: 10.0, // More than available
            price: 400.0,
            offset: Offset::Close,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        };

        let result = holding.convert_order_request_net(&req);

        // Should have CloseToday, CloseYesterday, and Open for remaining
        assert!(result.len() >= 2);
        
        let total_volume: f64 = result.iter().map(|r| r.volume).sum();
        assert!((total_volume - 10.0).abs() < 0.01);
    }
}
