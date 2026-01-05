//! Offset converter for handling position offset in different exchanges.

use std::collections::HashMap;

use super::constant::{Direction, Exchange, Offset};
use super::object::{ContractData, OrderData, OrderRequest, PositionData, TradeData};

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
        if position.direction == Direction::Long {
            self.long_pos = position.volume;
            self.long_yd = position.yd_volume;
            self.long_td = self.long_pos - self.long_yd;
        } else {
            self.short_pos = position.volume;
            self.short_yd = position.yd_volume;
            self.short_td = self.short_pos - self.short_yd;
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
                _ => {}
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
                _ => {}
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
                    _ => {}
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
                    _ => {}
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
            return vec![];
        } else if req.volume <= td_available {
            let mut req_td = req.clone();
            req_td.offset = Offset::CloseToday;
            return vec![req_td];
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

            return req_list;
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
            return vec![req_open];
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

            return req_list;
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

            return reqs;
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

            return reqs;
        }
    }
}

/// Offset converter for managing position holdings and order conversion
pub struct OffsetConverter<F>
where
    F: Fn(&str) -> Option<ContractData>,
{
    holdings: HashMap<String, PositionHolding>,
    get_contract: F,
}

impl<F> OffsetConverter<F>
where
    F: Fn(&str) -> Option<ContractData>,
{
    /// Create a new OffsetConverter
    pub fn new(get_contract: F) -> Self {
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
    fn is_convert_required(&self, vt_symbol: &str) -> bool {
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

    #[test]
    fn test_position_holding_new() {
        let contract = ContractData::new(
            "test".to_string(),
            "IF2312".to_string(),
            Exchange::Cffex,
            "沪深300指数期货".to_string(),
            super::super::constant::Product::Futures,
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
            super::super::constant::Product::Futures,
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
}
