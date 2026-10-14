use crate::error::SimError;
use crate::ledger::Ledger;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Shop economy (design/05-06): stock with prices, till as a ledger account,
/// catalogue auto-order. Money always moves through the closed ledger —
/// buying is a transfer, restocking pays a supplier account.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stock {
    pub price_cents: i64,
    pub qty: u32,
    pub restock_at: u32,
    pub restock_qty: u32,
    pub restock_cost_cents: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Shop {
    pub till: u64,
    pub stock: BTreeMap<String, Stock>,
    /// Opening hour (inclusive) in game hours. Default 0 for old saves.
    #[serde(default)]
    pub open_hour: u8,
    /// Closing hour (exclusive) in game hours. Default 24 (open all day).
    #[serde(default = "close_of_day")]
    pub close_hour: u8,
}

fn close_of_day() -> u8 {
    24
}

impl Shop {
    pub fn new(till: u64) -> Self {
        Self {
            till,
            stock: BTreeMap::new(),
            open_hour: 0,
            close_hour: close_of_day(),
        }
    }

    pub fn add_line(
        &mut self,
        item: &str,
        price_cents: i64,
        qty: u32,
        restock_at: u32,
        restock_qty: u32,
        restock_cost_cents: i64,
    ) {
        self.stock.insert(
            item.to_string(),
            Stock {
                price_cents,
                qty,
                restock_at,
                restock_qty,
                restock_cost_cents,
            },
        );
    }

    /// Customer purchase: transfer price buyer→till, decrement stock.
    pub fn buy(&mut self, ledger: &mut Ledger, buyer: u64, item: &str) -> Result<(), SimError> {
        self.buy_with_cash(ledger, buyer, 0, item)
    }

    /// Purchase split across bank transfer and pocket cash. The cash part
    /// enters the books as a logged mint to the till (a cash deposit), so
    /// the ledger stays closed; pocket accounting stays caller-side.
    /// Fails before moving anything when the bank part overdraws.
    pub fn buy_with_cash(
        &mut self,
        ledger: &mut Ledger,
        buyer: u64,
        cash: i64,
        item: &str,
    ) -> Result<(), SimError> {
        if cash < 0 {
            return Err(SimError::new("neg_amt", "cash >= 0", "negative"));
        }
        let line =
            self.stock
                .get_mut(item)
                .ok_or(SimError::new("no_line", "stocked item", "unknown"))?;
        if line.qty == 0 {
            return Err(SimError::new("sold_out", "qty > 0", "empty"));
        }
        if cash > line.price_cents {
            // Overpaying cash is a shop error, not a tip: refuse cleanly.
            return Err(SimError::new("overpay", "cash <= price", "too much"));
        }
        // Fails before moving anything when the bank part overdraws.
        let bank_part = (line.price_cents - cash).max(0);
        if bank_part > 0 {
            ledger.transfer(buyer, self.till, bank_part)?;
        }
        if cash > 0 {
            ledger.mint(self.till, cash)?;
        }
        line.qty -= 1;
        Ok(())
    }

    /// Restock one line: till pays the supplier, qty tops up.
    pub fn restock(
        &mut self,
        ledger: &mut Ledger,
        supplier: u64,
        item: &str,
    ) -> Result<(), SimError> {
        let line =
            self.stock
                .get_mut(item)
                .ok_or(SimError::new("no_line", "stocked item", "unknown"))?;
        ledger.transfer(self.till, supplier, line.restock_cost_cents)?;
        line.qty += line.restock_qty;
        Ok(())
    }

    /// Opening hours in game hours, half-open [open, close) on the
    /// 24-hour clock. Overnight ranges (close <= open) stay open past
    /// midnight; 0–24 never shuts.
    pub fn set_hours(&mut self, open: u8, close: u8) {
        self.open_hour = open.min(24);
        self.close_hour = close.min(24);
    }

    /// True when the counter serves at this game hour.
    pub fn is_open(&self, hour: u8) -> bool {
        if self.open_hour == 0 && self.close_hour == 24 {
            return true;
        }
        if self.close_hour <= self.open_hour {
            hour >= self.open_hour || hour < self.close_hour
        } else {
            hour >= self.open_hour && hour < self.close_hour
        }
    }

    /// Catalogue auto-order: restock every line below threshold that the
    /// till can cover. Returns restocked item ids.
    pub fn auto_order(&mut self, ledger: &mut Ledger, supplier: u64) -> Vec<String> {
        let mut done = Vec::new();
        let ids: Vec<String> = self.stock.keys().cloned().collect();
        for id in ids {
            let (low, affordable) = match self.stock.get(&id) {
                Some(l) => (
                    l.qty < l.restock_at,
                    ledger.balance(self.till).unwrap_or(0) >= l.restock_cost_cents,
                ),
                None => continue,
            };
            if low && affordable && self.restock(ledger, supplier, &id).is_ok() {
                done.push(id);
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{EntityId, HostId};

    fn setup() -> (Ledger, u64, u64, u64, Shop) {
        let mut l = Ledger::new();
        let buyer = l.open(EntityId(1), HostId(1));
        let supplier = l.open(EntityId(9), HostId(9));
        let till = l.open(EntityId(2), HostId(2));
        l.mint(buyer, 1000).unwrap();
        l.mint(till, 500).unwrap();
        let mut s = Shop::new(till);
        s.add_line("ration", 100, 2, 2, 5, 200);
        s.add_line("meds", 400, 0, 1, 3, 900);
        (l, buyer, supplier, till, s)
    }

    #[test]
    fn buy_moves_money_and_stock() {
        let (mut l, buyer, _, till, mut s) = setup();
        s.buy(&mut l, buyer, "ration").unwrap();
        assert_eq!(l.balance(buyer).unwrap(), 900);
        assert_eq!(l.balance(till).unwrap(), 600);
        assert_eq!(s.stock["ration"].qty, 1);
        assert_eq!(l.live_sum(), l.audit_sum());
    }

    #[test]
    fn buy_with_cash_splits_bank_and_deposit() {
        let (mut l, buyer, _, till, mut s) = setup();
        // Refusals first, while ration is stocked: overpay, negative cash.
        assert!(s.buy_with_cash(&mut l, buyer, 101, "ration").is_err());
        assert!(s.buy_with_cash(&mut l, buyer, -1, "ration").is_err());
        assert_eq!(s.stock["ration"].qty, 2);
        // Bank covers 60 of 100; 40 cash enters the books as till mint.
        s.buy_with_cash(&mut l, buyer, 40, "ration").unwrap();
        assert_eq!(l.balance(buyer).unwrap(), 940);
        assert_eq!(l.balance(till).unwrap(), 600);
        assert_eq!(s.stock["ration"].qty, 1);
        assert_eq!(l.live_sum(), l.audit_sum());
        // Full cash: bank untouched, till minted the price.
        s.buy_with_cash(&mut l, buyer, 100, "ration").unwrap();
        assert_eq!(l.balance(buyer).unwrap(), 940);
        assert_eq!(l.balance(till).unwrap(), 700);
        assert_eq!(s.stock["ration"].qty, 0);
        assert_eq!(l.live_sum(), l.audit_sum());
        // Sold out refuses even with exact cash.
        assert!(s.buy_with_cash(&mut l, buyer, 100, "ration").is_err());
        assert_eq!(l.live_sum(), l.audit_sum());
    }

    #[test]
    fn opening_hours_gate_service() {
        let (_, _, _, till, mut s) = setup();
        // Default construction serves round the clock (legacy behavior).
        let fresh = Shop::new(till);
        assert!(fresh.is_open(0) && fresh.is_open(12) && fresh.is_open(23));
        // Day shop 8-20: boundary hours inclusive start, exclusive end.
        s.set_hours(8, 20);
        assert!(!s.is_open(7));
        assert!(s.is_open(8));
        assert!(s.is_open(19));
        assert!(!s.is_open(20));
        assert!(!s.is_open(0));
        // Overnight range wraps past midnight.
        s.set_hours(20, 4);
        assert!(s.is_open(22));
        assert!(s.is_open(0));
        assert!(!s.is_open(10));
        // Clamp garbage to the 24-hour dial.
        s.set_hours(30, 99);
        assert_eq!((s.open_hour, s.close_hour), (24, 24));
    }

    #[test]
    fn sold_out_unknown_and_broke_rejected() {
        let (mut l, buyer, _, _, mut s) = setup();
        assert!(s.buy(&mut l, buyer, "meds").is_err()); // qty 0
        assert!(s.buy(&mut l, buyer, "laser").is_err()); // unknown
        let poor = l.open(EntityId(3), HostId(3));
        assert!(s.buy(&mut l, poor, "ration").is_err()); // overdraw
        assert_eq!(s.stock["ration"].qty, 2); // untouched
    }

    #[test]
    fn auto_order_tops_up_only_funded_lines() {
        let (mut l, _, supplier, _, mut s) = setup();
        // ration qty 2 vs restock_at 2 → not low (strict <). Nothing to do.
        assert!(s.auto_order(&mut l, supplier).is_empty());
        // Sell ration out, then auto-order: till covers it, meds stay skipped.
        let rich = l.open(EntityId(5), HostId(5));
        l.mint(rich, 10000).unwrap();
        s.buy(&mut l, rich, "ration").unwrap();
        s.buy(&mut l, rich, "ration").unwrap();
        let done = s.auto_order(&mut l, supplier);
        assert_eq!(done, vec!["ration".to_string()]);
        assert_eq!(s.stock["ration"].qty, 5);
        assert_eq!(l.live_sum(), l.audit_sum());
    }
}
