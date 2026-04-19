//! Bracket/OCO/OTO contingent order engine.
//!
//! Manages groups of related orders with contingent execution logic:
//! - **Bracket**: Entry → TakeProfit + StopLoss (one-cancels-other on exit)
//! - **OCO**: Two orders where fill of one cancels the other
//! - **OTO**: Primary order fill triggers secondary order submission

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::constant::{Direction, Exchange, Offset, OrderType, Status};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::{CancelRequest, OrderData, OrderRequest, TradeData};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Type of contingent order relationship
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContingencyType {
    /// One-Cancels-the-Other
    Oco,
    /// One-Triggers-the-Other
    Oto,
    /// Bracket: entry + take-profit + stop-loss
    Bracket,
}

impl std::fmt::Display for ContingencyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContingencyType::Oco => write!(f, "OCO"),
            ContingencyType::Oto => write!(f, "OTO"),
            ContingencyType::Bracket => write!(f, "Bracket"),
        }
    }
}

/// State of an order group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderGroupState {
    /// Created but no orders submitted yet
    Pending,
    /// Entry / primary order is active
    EntryActive,
    /// Secondary (TP/SL or OCO) orders are active
    SecondaryActive,
    /// Group fully completed
    Completed,
    /// Group cancelled by user
    Cancelled,
    /// Entry / primary order rejected
    Rejected,
}

impl std::fmt::Display for OrderGroupState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderGroupState::Pending => write!(f, "Pending"),
            OrderGroupState::EntryActive => write!(f, "EntryActive"),
            OrderGroupState::SecondaryActive => write!(f, "SecondaryActive"),
            OrderGroupState::Completed => write!(f, "Completed"),
            OrderGroupState::Cancelled => write!(f, "Cancelled"),
            OrderGroupState::Rejected => write!(f, "Rejected"),
        }
    }
}

/// Role of a child order within its group
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OrderRole {
    /// Bracket entry order
    Entry,
    /// Bracket take-profit order
    TakeProfit,
    /// Bracket stop-loss order
    StopLoss,
    /// OTO primary order
    Primary,
    /// OTO secondary order
    Secondary,
    /// OCO order A
    OrderA,
    /// OCO order B
    OrderB,
}

impl std::fmt::Display for OrderRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderRole::Entry => write!(f, "Entry"),
            OrderRole::TakeProfit => write!(f, "TakeProfit"),
            OrderRole::StopLoss => write!(f, "StopLoss"),
            OrderRole::Primary => write!(f, "Primary"),
            OrderRole::Secondary => write!(f, "Secondary"),
            OrderRole::OrderA => write!(f, "OrderA"),
            OrderRole::OrderB => write!(f, "OrderB"),
        }
    }
}

fn role_key(role: OrderRole) -> String {
    format!("{:?}", role)
}

// ---------------------------------------------------------------------------
// Unique group ID
// ---------------------------------------------------------------------------

/// Unique bracket order group identifier
pub type GroupId = u64;

// ---------------------------------------------------------------------------
// Child order & order group
// ---------------------------------------------------------------------------

/// A single order within a contingent order group
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildOrder {
    pub role: OrderRole,
    pub request: OrderRequest,
    pub vt_orderid: Option<String>,
    pub status: Status,
    pub filled_volume: f64,
    pub avg_fill_price: f64,
}

impl ChildOrder {
    /// Whether the child order is still active (may receive fills)
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            Status::Submitting | Status::NotTraded | Status::PartTraded
        )
    }

    /// Whether the child order is fully filled
    pub fn is_fully_filled(&self) -> bool {
        self.status == Status::AllTraded
    }
}

/// A group of contingent orders
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderGroup {
    pub id: GroupId,
    pub contingency_type: ContingencyType,
    pub state: OrderGroupState,
    pub vt_symbol: String,
    pub gateway_name: String,
    pub orders: HashMap<String, ChildOrder>,
    pub reference: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub tag: String,
}

impl OrderGroup {
    /// Whether this group is still active (may produce further fills)
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            OrderGroupState::Pending
                | OrderGroupState::EntryActive
                | OrderGroupState::SecondaryActive
        )
    }
}

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Request to create a bracket order (entry + TP + SL)
#[derive(Debug, Clone)]
pub struct BracketOrderRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub entry_price: f64,
    pub entry_volume: f64,
    pub entry_type: OrderType,
    pub tp_price: f64,
    pub sl_price: f64,
    pub sl_type: OrderType,
    pub offset: Offset,
    pub gateway_name: String,
    pub reference: String,
    pub tag: String,
}

/// Request to create an OCO (one-cancels-other) order pair
#[derive(Debug, Clone)]
pub struct OcoOrderRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub direction: Direction,
    pub volume: f64,
    pub order_a_price: f64,
    pub order_a_type: OrderType,
    pub order_b_price: f64,
    pub order_b_type: OrderType,
    pub offset: Offset,
    pub gateway_name: String,
    pub reference: String,
    pub tag: String,
}

/// Request to create an OTO (one-triggers-other) order pair
#[derive(Debug, Clone)]
pub struct OtoOrderRequest {
    pub symbol: String,
    pub exchange: Exchange,
    pub primary_direction: Direction,
    pub primary_price: f64,
    pub primary_volume: f64,
    pub primary_type: OrderType,
    pub secondary_direction: Direction,
    pub secondary_price: f64,
    pub secondary_volume: f64,
    pub secondary_type: OrderType,
    pub offset: Offset,
    pub gateway_name: String,
    pub reference: String,
    pub tag: String,
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

/// Callback invoked to send an order through the OMS / gateway.
/// Returns the vt_orderid on success.
pub type SendOrderCallback = Box<dyn Fn(&OrderRequest) -> Result<String, String> + Send + Sync>;

/// Callback invoked to cancel an order through the OMS / gateway.
pub type CancelOrderCallback = Box<dyn Fn(&CancelRequest) -> Result<(), String> + Send + Sync>;

/// Callback invoked when an order group changes state.
pub type StateChangeCallback = Box<dyn Fn(&OrderGroup) + Send + Sync>;

// ---------------------------------------------------------------------------
// BracketOrderEngine
// ---------------------------------------------------------------------------

/// Bracket/OCO/OTO contingent order engine.
pub struct BracketOrderEngine {
    name: String,
    groups: RwLock<HashMap<GroupId, OrderGroup>>,
    orderid_to_group: RwLock<HashMap<String, GroupId>>,
    next_id: AtomicU64,
    running: AtomicBool,
    send_order_callback: Arc<RwLock<Option<SendOrderCallback>>>,
    cancel_order_callback: Arc<RwLock<Option<CancelOrderCallback>>>,
    state_change_callback: Arc<RwLock<Option<StateChangeCallback>>>,
}

impl BracketOrderEngine {
    pub fn new() -> Self {
        Self {
            name: "BracketOrderEngine".to_string(),
            groups: RwLock::new(HashMap::new()),
            orderid_to_group: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            running: AtomicBool::new(false),
            send_order_callback: Arc::new(RwLock::new(None)),
            cancel_order_callback: Arc::new(RwLock::new(None)),
            state_change_callback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_send_order_callback(&self, cb: SendOrderCallback) {
        let mut slot = self.send_order_callback.write().unwrap_or_else(|e| e.into_inner());
        *slot = Some(cb);
    }

    pub fn set_cancel_order_callback(&self, cb: CancelOrderCallback) {
        let mut slot = self.cancel_order_callback.write().unwrap_or_else(|e| e.into_inner());
        *slot = Some(cb);
    }

    pub fn set_state_change_callback(&self, cb: StateChangeCallback) {
        let mut slot = self.state_change_callback.write().unwrap_or_else(|e| e.into_inner());
        *slot = Some(cb);
    }

    fn close_direction(dir: Direction) -> Direction {
        match dir {
            Direction::Long => Direction::Short,
            Direction::Short => Direction::Long,
            Direction::Net => Direction::Net,
        }
    }

    pub fn add_bracket_order(&self, req: BracketOrderRequest) -> Result<GroupId, String> {
        if req.entry_volume <= 0.0 { return Err("委托数量必须大于零".to_string()); }
        if req.tp_price <= 0.0 { return Err("止盈价格必须大于零".to_string()); }
        if req.sl_price <= 0.0 { return Err("止损价格必须大于零".to_string()); }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());
        let close_dir = Self::close_direction(req.direction);

        let entry_request = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.direction,
            order_type: req.entry_type,
            volume: req.entry_volume,
            price: req.entry_price,
            offset: req.offset,
            reference: if req.reference.is_empty() { format!("BRACKET_{}_ENTRY", id) } else { req.reference.clone() },
            post_only: false,
            reduce_only: false,
        };
        let entry_child = ChildOrder {
            role: OrderRole::Entry, request: entry_request.clone(),
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };

        let tp_request = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: close_dir,
            order_type: OrderType::Limit,
            volume: req.entry_volume,
            price: req.tp_price,
            offset: req.offset,
            reference: format!("BRACKET_{}_TP", id),
            post_only: false,
            reduce_only: false,
        };
        let tp_child = ChildOrder {
            role: OrderRole::TakeProfit, request: tp_request,
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };

        let sl_request = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: close_dir,
            order_type: req.sl_type,
            volume: req.entry_volume,
            price: if req.sl_type == OrderType::Stop || req.sl_type == OrderType::StopLimit { req.sl_price } else { 0.0 },
            offset: req.offset,
            reference: format!("BRACKET_{}_SL", id),
            post_only: false,
            reduce_only: false,
        };
        let sl_child = ChildOrder {
            role: OrderRole::StopLoss, request: sl_request,
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };

        let mut orders = HashMap::new();
        orders.insert(role_key(OrderRole::Entry), entry_child);
        orders.insert(role_key(OrderRole::TakeProfit), tp_child);
        orders.insert(role_key(OrderRole::StopLoss), sl_child);

        let group = OrderGroup {
            id, contingency_type: ContingencyType::Bracket, state: OrderGroupState::Pending,
            vt_symbol: vt_symbol.clone(), gateway_name: req.gateway_name.clone(), orders,
            reference: req.reference.clone(), created_at: Utc::now(), completed_at: None,
            tag: req.tag.clone(),
        };

        { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner()); g.insert(id, group); }
        self.submit_entry_order(id, &entry_request);
        info!("[BracketOrderEngine] 新增Bracket委托组 #{} {} entry={}", id, vt_symbol, req.entry_price);
        Ok(id)
    }

    pub fn add_oco_order(&self, req: OcoOrderRequest) -> Result<GroupId, String> {
        if req.volume <= 0.0 { return Err("委托数量必须大于零".to_string()); }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());

        let a_req = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.direction,
            order_type: req.order_a_type,
            volume: req.volume,
            price: req.order_a_price,
            offset: req.offset,
            reference: if req.reference.is_empty() { format!("OCO_{}_A", id) } else { req.reference.clone() },
            post_only: false,
            reduce_only: false,
        };
        let b_req = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.direction,
            order_type: req.order_b_type,
            volume: req.volume,
            price: req.order_b_price,
            offset: req.offset,
            reference: format!("OCO_{}_B", id),
            post_only: false,
            reduce_only: false,
        };

        let child_a = ChildOrder {
            role: OrderRole::OrderA, request: a_req.clone(),
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };
        let child_b = ChildOrder {
            role: OrderRole::OrderB, request: b_req.clone(),
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };

        let mut orders = HashMap::new();
        orders.insert(role_key(OrderRole::OrderA), child_a);
        orders.insert(role_key(OrderRole::OrderB), child_b);

        let group = OrderGroup {
            id, contingency_type: ContingencyType::Oco, state: OrderGroupState::Pending,
            vt_symbol: vt_symbol.clone(), gateway_name: req.gateway_name.clone(), orders,
            reference: req.reference.clone(), created_at: Utc::now(), completed_at: None,
            tag: req.tag.clone(),
        };

        { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner()); g.insert(id, group); }
        self.submit_child_order(id, &a_req, OrderRole::OrderA);
        self.submit_child_order(id, &b_req, OrderRole::OrderB);

        { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
          if let Some(gr) = g.get_mut(&id) { gr.state = OrderGroupState::SecondaryActive; } }
        self.fire_state_change(id);
        info!("[BracketOrderEngine] 新增OCO委托组 #{} {} A={}/B={}", id, vt_symbol, req.order_a_price, req.order_b_price);
        Ok(id)
    }

    pub fn add_oto_order(&self, req: OtoOrderRequest) -> Result<GroupId, String> {
        if req.primary_volume <= 0.0 { return Err("主委托数量必须大于零".to_string()); }
        if req.secondary_volume <= 0.0 { return Err("次委托数量必须大于零".to_string()); }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let vt_symbol = format!("{}.{}", req.symbol, req.exchange.value());

        let p_req = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.primary_direction,
            order_type: req.primary_type,
            volume: req.primary_volume,
            price: req.primary_price,
            offset: req.offset,
            reference: if req.reference.is_empty() { format!("OTO_{}_PRIMARY", id) } else { req.reference.clone() },
            post_only: false,
            reduce_only: false,
        };
        let s_req = OrderRequest {
            symbol: req.symbol.clone(),
            exchange: req.exchange,
            direction: req.secondary_direction,
            order_type: req.secondary_type,
            volume: req.secondary_volume,
            price: req.secondary_price,
            offset: req.offset,
            reference: format!("OTO_{}_SECONDARY", id),
            post_only: false,
            reduce_only: false,
        };

        let p_child = ChildOrder {
            role: OrderRole::Primary, request: p_req.clone(),
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };
        let s_child = ChildOrder {
            role: OrderRole::Secondary, request: s_req,
            vt_orderid: None, status: Status::Submitting, filled_volume: 0.0, avg_fill_price: 0.0,
        };

        let mut orders = HashMap::new();
        orders.insert(role_key(OrderRole::Primary), p_child);
        orders.insert(role_key(OrderRole::Secondary), s_child);

        let group = OrderGroup {
            id, contingency_type: ContingencyType::Oto, state: OrderGroupState::Pending,
            vt_symbol: vt_symbol.clone(), gateway_name: req.gateway_name.clone(), orders,
            reference: req.reference.clone(), created_at: Utc::now(), completed_at: None,
            tag: req.tag.clone(),
        };

        { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner()); g.insert(id, group); }
        self.submit_entry_order(id, &p_req);
        info!("[BracketOrderEngine] 新增OTO委托组 #{} {} primary={}", id, vt_symbol, req.primary_price);
        Ok(id)
    }

    pub fn cancel_group(&self, group_id: GroupId) -> Result<(), String> {
        let vt_orderids: Vec<String> = {
            let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
            let group = groups.get_mut(&group_id)
                .ok_or_else(|| format!("委托组 #{} 不存在", group_id))?;
            if !group.is_active() {
                return Err(format!("委托组 #{} 状态为{}，无法取消", group_id, group.state));
            }
            group.state = OrderGroupState::Cancelled;
            group.completed_at = Some(Utc::now());
            group.orders.values().filter(|c| c.is_active()).filter_map(|c| c.vt_orderid.clone()).collect()
        };
        for vtid in vt_orderids { self.cancel_child_order(&vtid); }
        self.fire_state_change(group_id);
        info!("[BracketOrderEngine] 取消委托组 #{}", group_id);
        Ok(())
    }

    pub fn get_group(&self, id: GroupId) -> Option<OrderGroup> {
        self.groups.read().unwrap_or_else(|e| e.into_inner()).get(&id).cloned()
    }

    pub fn get_all_groups(&self) -> Vec<OrderGroup> {
        self.groups.read().unwrap_or_else(|e| e.into_inner()).values().cloned().collect()
    }

    pub fn get_active_groups(&self) -> Vec<OrderGroup> {
        self.groups.read().unwrap_or_else(|e| e.into_inner()).values().filter(|g| g.is_active()).cloned().collect()
    }

    pub fn cleanup(&self) {
        let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
        let mut oid_map = self.orderid_to_group.write().unwrap_or_else(|e| e.into_inner());
        let inactive: Vec<GroupId> = groups.iter().filter(|(_, g)| !g.is_active()).map(|(id, _)| *id).collect();
        for id in &inactive {
            if let Some(group) = groups.remove(id) {
                for child in group.orders.values() {
                    if let Some(ref vtid) = child.vt_orderid { oid_map.remove(vtid); }
                }
            }
        }
        if !inactive.is_empty() { info!("[BracketOrderEngine] 清理{}个非活跃委托组", inactive.len()); }
    }

    // -- Private helpers --

    fn submit_entry_order(&self, group_id: GroupId, req: &OrderRequest) {
        let cb = self.send_order_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref send) = *cb {
            match send(req) {
                Ok(vt_orderid) => {
                    info!("[BracketOrderEngine] 委托下单 组#{} -> {}", group_id, vt_orderid);
                    let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
                    let mut oid_map = self.orderid_to_group.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(group) = groups.get_mut(&group_id) {
                        if let Some(child) = group.orders.get_mut(&role_key(OrderRole::Entry)) {
                            child.vt_orderid = Some(vt_orderid.clone());
                            child.status = Status::NotTraded;
                            oid_map.insert(vt_orderid.clone(), group_id);
                        }
                        if let Some(child) = group.orders.get_mut(&role_key(OrderRole::Primary)) {
                            child.vt_orderid = Some(vt_orderid.clone());
                            child.status = Status::NotTraded;
                            oid_map.insert(vt_orderid, group_id);
                        }
                        if group.state == OrderGroupState::Pending {
                            group.state = OrderGroupState::EntryActive;
                        }
                    }
                    drop(groups); drop(oid_map);
                    self.fire_state_change(group_id);
                }
                Err(e) => {
                    warn!("[BracketOrderEngine] 委托下单失败 组#{}: {}", group_id, e);
                    let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(group) = groups.get_mut(&group_id) {
                        group.state = OrderGroupState::Rejected;
                        group.completed_at = Some(Utc::now());
                    }
                    drop(groups);
                    self.fire_state_change(group_id);
                }
            }
        }
    }

    fn submit_child_order(&self, group_id: GroupId, req: &OrderRequest, role: OrderRole) {
        let cb = self.send_order_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref send) = *cb {
            match send(req) {
                Ok(vt_orderid) => {
                    info!("[BracketOrderEngine] 委托下单 组#{} {} -> {}", group_id, role, vt_orderid);
                    let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
                    let mut oid_map = self.orderid_to_group.write().unwrap_or_else(|e| e.into_inner());
                    if let Some(group) = groups.get_mut(&group_id) {
                        if let Some(child) = group.orders.get_mut(&role_key(role)) {
                            child.vt_orderid = Some(vt_orderid.clone());
                            child.status = Status::NotTraded;
                            oid_map.insert(vt_orderid, group_id);
                        }
                    }
                }
                Err(e) => { warn!("[BracketOrderEngine] 委托下单失败 组#{} {}: {}", group_id, role, e); }
            }
        }
    }

    fn cancel_child_order(&self, vt_orderid: &str) {
        let cb = self.cancel_order_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cancel) = *cb {
            let orderid = vt_orderid.rsplit_once('.').map(|(_, id)| id).unwrap_or(vt_orderid);
            let (symbol, exchange) = {
                let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                let oid_map = self.orderid_to_group.read().unwrap_or_else(|e| e.into_inner());
                if let Some(gid) = oid_map.get(vt_orderid) {
                    if let Some(group) = groups.get(gid) {
                        if let Some(child) = group.orders.values().find(|c| c.vt_orderid.as_deref() == Some(vt_orderid)) {
                            (child.request.symbol.clone(), child.request.exchange)
                        } else { return; }
                    } else { return; }
                } else { return; }
            };
            let cancel_req = CancelRequest { orderid: orderid.to_string(), symbol, exchange };
            match cancel(&cancel_req) {
                Ok(()) => info!("[BracketOrderEngine] 委托撤单 {}", vt_orderid),
                Err(e) => warn!("[BracketOrderEngine] 委托撤单失败 {}: {}", vt_orderid, e),
            }
        }
    }

    fn process_order_update(&self, order: &OrderData) {
        let vt_orderid = order.vt_orderid();
        let group_id = {
            let m = self.orderid_to_group.read().unwrap_or_else(|e| e.into_inner());
            match m.get(&vt_orderid).copied() { Some(id) => id, None => return }
        };
        {
            let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
            if let Some(group) = groups.get_mut(&group_id) {
                if let Some(child) = group.orders.values_mut().find(|c| c.vt_orderid.as_deref() == Some(&vt_orderid)) {
                    child.status = order.status;
                    child.filled_volume = order.traded;
                }
            }
        }
        match order.status {
            Status::AllTraded => self.handle_fill(&vt_orderid, group_id, order.traded, order.price),
            Status::Rejected => self.handle_rejection(&vt_orderid, group_id),
            Status::Cancelled => self.handle_cancellation(&vt_orderid, group_id),
            _ => {}
        }
    }

    fn process_trade(&self, trade: &TradeData) {
        let vt_orderid = trade.vt_orderid();
        let group_id = {
            let m = self.orderid_to_group.read().unwrap_or_else(|e| e.into_inner());
            match m.get(&vt_orderid).copied() { Some(id) => id, None => return }
        };
        {
            let mut groups = self.groups.write().unwrap_or_else(|e| e.into_inner());
            if let Some(group) = groups.get_mut(&group_id) {
                if let Some(child) = group.orders.values_mut().find(|c| c.vt_orderid.as_deref() == Some(&vt_orderid)) {
                    let prev = child.filled_volume;
                    let nv = trade.volume;
                    let total = prev + nv;
                    if total > 0.0 { child.avg_fill_price = (child.avg_fill_price * prev + trade.price * nv) / total; }
                    child.filled_volume = total;
                }
            }
        }
    }

    fn handle_fill(&self, vt_orderid: &str, group_id: GroupId, filled: f64, _fill_price: f64) {
        let ct = { self.groups.read().unwrap_or_else(|e| e.into_inner()).get(&group_id).map(|g| g.contingency_type) };
        match ct {
            Some(ContingencyType::Bracket) => self.handle_bracket_fill(vt_orderid, group_id, filled),
            Some(ContingencyType::Oco) => self.handle_oco_fill(vt_orderid, group_id),
            Some(ContingencyType::Oto) => self.handle_oto_fill(vt_orderid, group_id, filled),
            None => {}
        }
    }

    fn find_role(&self, group_id: GroupId, vt_orderid: &str) -> Option<OrderRole> {
        self.groups.read().unwrap_or_else(|e| e.into_inner()).get(&group_id).and_then(|g| {
            g.orders.values().find(|c| c.vt_orderid.as_deref() == Some(vt_orderid)).map(|c| c.role)
        })
    }

    fn handle_bracket_fill(&self, vt_orderid: &str, group_id: GroupId, filled: f64) {
        let role = self.find_role(group_id, vt_orderid);
        match role {
            Some(OrderRole::Entry) => {
                let (tp_req, sl_req) = {
                    let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                    if let Some(group) = groups.get(&group_id) {
                        let mut tp = group.orders.get(&role_key(OrderRole::TakeProfit)).map(|c| c.request.clone());
                        let mut sl = group.orders.get(&role_key(OrderRole::StopLoss)).map(|c| c.request.clone());
                        if let Some(ref mut r) = tp { r.volume = filled; }
                        if let Some(ref mut r) = sl { r.volume = filled; }
                        (tp, sl)
                    } else { return; }
                };
                if let Some(ref r) = tp_req { self.submit_child_order(group_id, r, OrderRole::TakeProfit); }
                if let Some(ref r) = sl_req { self.submit_child_order(group_id, r, OrderRole::StopLoss); }
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::SecondaryActive; } }
                self.fire_state_change(group_id);
            }
            Some(OrderRole::TakeProfit) | Some(OrderRole::StopLoss) => {
                let sibling_role = if role == Some(OrderRole::TakeProfit) { OrderRole::StopLoss } else { OrderRole::TakeProfit };
                let sibling_id = {
                    let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                    groups.get(&group_id).and_then(|g| g.orders.get(&role_key(sibling_role)).and_then(|c| c.vt_orderid.clone()))
                };
                if let Some(ref sid) = sibling_id { self.cancel_child_order(sid); }
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Completed; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
            }
            _ => {}
        }
    }

    fn handle_oco_fill(&self, vt_orderid: &str, group_id: GroupId) {
        let filled_role = self.find_role(group_id, vt_orderid);
        let sibling_role = match filled_role {
            Some(OrderRole::OrderA) => OrderRole::OrderB,
            Some(OrderRole::OrderB) => OrderRole::OrderA,
            _ => return,
        };
        let sibling_id = {
            let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
            groups.get(&group_id).and_then(|g| g.orders.get(&role_key(sibling_role)).and_then(|c| c.vt_orderid.clone()))
        };
        if let Some(ref sid) = sibling_id { self.cancel_child_order(sid); }
        { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
          if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Completed; gr.completed_at = Some(Utc::now()); } }
        self.fire_state_change(group_id);
    }

    fn handle_oto_fill(&self, vt_orderid: &str, group_id: GroupId, _filled: f64) {
        let role = self.find_role(group_id, vt_orderid);
        match role {
            Some(OrderRole::Primary) => {
                let sec_req = {
                    let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                    groups.get(&group_id).and_then(|g| g.orders.get(&role_key(OrderRole::Secondary)).map(|c| c.request.clone()))
                };
                if let Some(ref r) = sec_req { self.submit_child_order(group_id, r, OrderRole::Secondary); }
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::SecondaryActive; } }
                self.fire_state_change(group_id);
            }
            Some(OrderRole::Secondary) => {
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Completed; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
            }
            _ => {}
        }
    }

    fn handle_rejection(&self, vt_orderid: &str, group_id: GroupId) {
        let role = self.find_role(group_id, vt_orderid);
        match role {
            Some(OrderRole::Entry) | Some(OrderRole::Primary) => {
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Rejected; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
            }
            _ => { warn!("[BracketOrderEngine] 子委托被拒 组#{} {}", group_id, vt_orderid); }
        }
    }

    fn handle_cancellation(&self, vt_orderid: &str, group_id: GroupId) {
        let role = self.find_role(group_id, vt_orderid);
        match role {
            Some(OrderRole::Entry) | Some(OrderRole::Primary) => {
                // Entry/Primary cancelled → mark group Cancelled
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Cancelled; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
                info!("[BracketOrderEngine] 入场委托被撤销，组#{}已取消", group_id);
            }
            Some(OrderRole::TakeProfit) | Some(OrderRole::StopLoss) => {
                // One exit leg cancelled externally — cancel sibling and mark group Cancelled
                let sibling_role = if role == Some(OrderRole::TakeProfit) { OrderRole::StopLoss } else { OrderRole::TakeProfit };
                let sibling_id = {
                    let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                    groups.get(&group_id).and_then(|g| g.orders.get(&role_key(sibling_role)).and_then(|c| c.vt_orderid.clone()))
                };
                if let Some(ref sid) = sibling_id { self.cancel_child_order(sid); }
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Cancelled; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
                warn!("[BracketOrderEngine] 出场委托被撤销，组#{}已取消", group_id);
            }
            Some(OrderRole::OrderA) | Some(OrderRole::OrderB) => {
                // OCO: one leg cancelled — cancel sibling and mark group Cancelled
                let sibling_role = if role == Some(OrderRole::OrderA) { OrderRole::OrderB } else { OrderRole::OrderA };
                let sibling_id = {
                    let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
                    groups.get(&group_id).and_then(|g| g.orders.get(&role_key(sibling_role)).and_then(|c| c.vt_orderid.clone()))
                };
                if let Some(ref sid) = sibling_id { self.cancel_child_order(sid); }
                { let mut g = self.groups.write().unwrap_or_else(|e| e.into_inner());
                  if let Some(gr) = g.get_mut(&group_id) { gr.state = OrderGroupState::Cancelled; gr.completed_at = Some(Utc::now()); } }
                self.fire_state_change(group_id);
                warn!("[BracketOrderEngine] OCO委托被撤销，组#{}已取消", group_id);
            }
            Some(OrderRole::Secondary) => {
                // OTO: secondary cancelled — log; group may still have open primary
                warn!("[BracketOrderEngine] 次委托被撤销 组#{} {}", group_id, vt_orderid);
            }
            _ => {}
        }
    }

    fn fire_state_change(&self, group_id: GroupId) {
        let cb = self.state_change_callback.read().unwrap_or_else(|e| e.into_inner());
        if let Some(ref cb) = *cb {
            let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
            if let Some(group) = groups.get(&group_id) { cb(group); }
        }
    }
}

impl Default for BracketOrderEngine {
    fn default() -> Self { Self::new() }
}

impl BaseEngine for BracketOrderEngine {
    fn engine_name(&self) -> &str { &self.name }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
        let active_ids: Vec<GroupId> = {
            let groups = self.groups.read().unwrap_or_else(|e| e.into_inner());
            groups.iter().filter(|(_, g)| g.is_active()).map(|(id, _)| *id).collect()
        };
        for id in active_ids { let _ = self.cancel_group(id); }
        info!("[BracketOrderEngine] 关闭并取消所有活跃委托组");
    }

    fn process_event(&self, _event_type: &str, event: &GatewayEvent) {
        match event {
            GatewayEvent::Order(order) => self.process_order_update(order),
            GatewayEvent::Trade(trade) => self.process_trade(trade),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::constant::Exchange;

    #[test]
    fn test_bracket_order_engine_new() {
        let engine = BracketOrderEngine::new();
        assert_eq!(engine.engine_name(), "BracketOrderEngine");
        assert!(engine.get_all_groups().is_empty());
        assert!(engine.get_active_groups().is_empty());
    }

    #[test]
    fn test_contingency_type_display() {
        assert_eq!(format!("{}", ContingencyType::Oco), "OCO");
        assert_eq!(format!("{}", ContingencyType::Oto), "OTO");
        assert_eq!(format!("{}", ContingencyType::Bracket), "Bracket");
    }

    #[test]
    fn test_order_group_state_display() {
        assert_eq!(format!("{}", OrderGroupState::Pending), "Pending");
        assert_eq!(format!("{}", OrderGroupState::EntryActive), "EntryActive");
        assert_eq!(format!("{}", OrderGroupState::SecondaryActive), "SecondaryActive");
        assert_eq!(format!("{}", OrderGroupState::Completed), "Completed");
        assert_eq!(format!("{}", OrderGroupState::Cancelled), "Cancelled");
        assert_eq!(format!("{}", OrderGroupState::Rejected), "Rejected");
    }

    #[test]
    fn test_order_role_keys() {
        assert_eq!(role_key(OrderRole::Entry), "Entry");
        assert_eq!(role_key(OrderRole::TakeProfit), "TakeProfit");
        assert_eq!(role_key(OrderRole::StopLoss), "StopLoss");
        assert_eq!(role_key(OrderRole::Primary), "Primary");
        assert_eq!(role_key(OrderRole::Secondary), "Secondary");
        assert_eq!(role_key(OrderRole::OrderA), "OrderA");
        assert_eq!(role_key(OrderRole::OrderB), "OrderB");
    }

    #[test]
    fn test_child_order_active_status() {
        let req = OrderRequest::new("BTCUSDT".to_string(), Exchange::Binance, Direction::Long, OrderType::Limit, 1.0);
        let child = ChildOrder {
            role: OrderRole::Entry, request: req, vt_orderid: None,
            status: Status::NotTraded, filled_volume: 0.0, avg_fill_price: 0.0,
        };
        assert!(child.is_active());
        assert!(!child.is_fully_filled());
    }

    #[test]
    fn test_child_order_fully_filled() {
        let req = OrderRequest::new("BTCUSDT".to_string(), Exchange::Binance, Direction::Long, OrderType::Limit, 1.0);
        let child = ChildOrder {
            role: OrderRole::Entry, request: req, vt_orderid: Some("GW.123".to_string()),
            status: Status::AllTraded, filled_volume: 1.0, avg_fill_price: 50000.0,
        };
        assert!(!child.is_active());
        assert!(child.is_fully_filled());
    }

    #[test]
    fn test_bracket_order_request_limit_entry() {
        let engine = BracketOrderEngine::new();
        let req = BracketOrderRequest {
            symbol: "BTCUSDT".to_string(), exchange: Exchange::Binance,
            direction: Direction::Long, entry_price: 50000.0, entry_volume: 0.1,
            entry_type: OrderType::Limit, tp_price: 55000.0, sl_price: 48000.0,
            sl_type: OrderType::Stop, offset: Offset::None,
            gateway_name: "BINANCE_SPOT".to_string(), reference: String::new(), tag: String::new(),
        };
        let result = engine.add_bracket_order(req);
        assert!(result.is_ok());
        let gid = result.unwrap();
        let group = engine.get_group(gid).unwrap();
        assert_eq!(group.contingency_type, ContingencyType::Bracket);
        assert_eq!(group.vt_symbol, "BTCUSDT.BINANCE");
        assert!(group.orders.contains_key(&role_key(OrderRole::Entry)));
        assert!(group.orders.contains_key(&role_key(OrderRole::TakeProfit)));
        assert!(group.orders.contains_key(&role_key(OrderRole::StopLoss)));
    }

    #[test]
    fn test_bracket_order_request_market_entry() {
        let engine = BracketOrderEngine::new();
        let req = BracketOrderRequest {
            symbol: "BTCUSDT".to_string(), exchange: Exchange::Binance,
            direction: Direction::Short, entry_price: 0.0, entry_volume: 0.1,
            entry_type: OrderType::Market, tp_price: 45000.0, sl_price: 52000.0,
            sl_type: OrderType::Stop, offset: Offset::None,
            gateway_name: "BINANCE_SPOT".to_string(), reference: String::new(), tag: String::new(),
        };
        let result = engine.add_bracket_order(req);
        assert!(result.is_ok());
        let group = engine.get_group(result.unwrap()).unwrap();
        let entry = group.orders.get(&role_key(OrderRole::Entry)).unwrap();
        assert_eq!(entry.request.order_type, OrderType::Market);
        assert_eq!(entry.request.direction, Direction::Short);
    }

    #[test]
    fn test_add_bracket_order_validation() {
        let engine = BracketOrderEngine::new();
        let req_zero_vol = BracketOrderRequest {
            symbol: "BTCUSDT".to_string(), exchange: Exchange::Binance,
            direction: Direction::Long, entry_price: 50000.0, entry_volume: 0.0,
            entry_type: OrderType::Limit, tp_price: 55000.0, sl_price: 48000.0,
            sl_type: OrderType::Stop, offset: Offset::None,
            gateway_name: "BINANCE_SPOT".to_string(), reference: String::new(), tag: String::new(),
        };
        assert!(engine.add_bracket_order(req_zero_vol).is_err());

        let req_zero_tp = BracketOrderRequest {
            symbol: "BTCUSDT".to_string(), exchange: Exchange::Binance,
            direction: Direction::Long, entry_price: 50000.0, entry_volume: 0.1,
            entry_type: OrderType::Limit, tp_price: 0.0, sl_price: 48000.0,
            sl_type: OrderType::Stop, offset: Offset::None,
            gateway_name: "BINANCE_SPOT".to_string(), reference: String::new(), tag: String::new(),
        };
        assert!(engine.add_bracket_order(req_zero_tp).is_err());

        let req_zero_sl = BracketOrderRequest {
            symbol: "BTCUSDT".to_string(), exchange: Exchange::Binance,
            direction: Direction::Long, entry_price: 50000.0, entry_volume: 0.1,
            entry_type: OrderType::Limit, tp_price: 55000.0, sl_price: 0.0,
            sl_type: OrderType::Stop, offset: Offset::None,
            gateway_name: "BINANCE_SPOT".to_string(), reference: String::new(), tag: String::new(),
        };
        assert!(engine.add_bracket_order(req_zero_sl).is_err());
    }

    #[test]
    fn test_cancel_nonexistent_group() {
        let engine = BracketOrderEngine::new();
        let result = engine.cancel_group(999);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_all_groups_empty() {
        let engine = BracketOrderEngine::new();
        assert!(engine.get_all_groups().is_empty());
    }

    #[test]
    fn test_get_active_groups_empty() {
        let engine = BracketOrderEngine::new();
        assert!(engine.get_active_groups().is_empty());
    }

    #[test]
    fn test_cleanup_no_active() {
        let engine = BracketOrderEngine::new();
        engine.cleanup();
        assert!(engine.get_all_groups().is_empty());
    }

    #[test]
    fn test_process_unrelated_event() {
        let engine = BracketOrderEngine::new();
        let tick = super::super::object::TickData::new(
            "BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, chrono::Utc::now(),
        );
        engine.process_event("tick", &GatewayEvent::Tick(tick));
        assert!(engine.get_all_groups().is_empty());

        let bar = super::super::object::BarData::new(
            "BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, chrono::Utc::now(),
        );
        engine.process_event("bar", &GatewayEvent::Bar(bar));
        assert!(engine.get_all_groups().is_empty());
    }
}
