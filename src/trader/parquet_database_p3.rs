
    fn ticks_to_dataframe(ticks: &[TickData]) -> Result<DataFrame, String> {
        let n = ticks.len();
        let mut datetimes = Vec::with_capacity(n);
        let mut symbols = Vec::with_capacity(n);
        let mut exchanges = Vec::with_capacity(n);
        let mut last_prices = Vec::with_capacity(n);
        let mut last_volumes = Vec::with_capacity(n);
        let mut volumes = Vec::with_capacity(n);
        let mut turnovers = Vec::with_capacity(n);
        let mut open_interests = Vec::with_capacity(n);
        let mut bid_price_1 = Vec::with_capacity(n);
        let mut bid_price_2 = Vec::with_capacity(n);
        let mut bid_price_3 = Vec::with_capacity(n);
        let mut bid_price_4 = Vec::with_capacity(n);
        let mut bid_price_5 = Vec::with_capacity(n);
        let mut ask_price_1 = Vec::with_capacity(n);
        let mut ask_price_2 = Vec::with_capacity(n);
        let mut ask_price_3 = Vec::with_capacity(n);
        let mut ask_price_4 = Vec::with_capacity(n);
        let mut ask_price_5 = Vec::with_capacity(n);
        let mut bid_volume_1 = Vec::with_capacity(n);
        let mut bid_volume_2 = Vec::with_capacity(n);
        let mut bid_volume_3 = Vec::with_capacity(n);
        let mut bid_volume_4 = Vec::with_capacity(n);
        let mut bid_volume_5 = Vec::with_capacity(n);
        let mut ask_volume_1 = Vec::with_capacity(n);
        let mut ask_volume_2 = Vec::with_capacity(n);
        let mut ask_volume_3 = Vec::with_capacity(n);
        let mut ask_volume_4 = Vec::with_capacity(n);
        let mut ask_volume_5 = Vec::with_capacity(n);
        let mut open_prices = Vec::with_capacity(n);
        let mut high_prices = Vec::with_capacity(n);
        let mut low_prices = Vec::with_capacity(n);
        let mut pre_closes = Vec::with_capacity(n);
        let mut limit_ups = Vec::with_capacity(n);
        let mut limit_downs = Vec::with_capacity(n);
        let mut gateway_name = Vec::with_capacity(n);

        for t in ticks {
            datetimes.push(t.datetime.timestamp_millis());
            symbols.push(t.symbol.clone());
            exchanges.push(t.exchange.value().to_string());
            last_prices.push(t.last_price);
            last_volumes.push(t.last_volume);
            volumes.push(t.volume);
            turnovers.push(t.turnover);
            open_interests.push(t.open_interest);
            bid_price_1.push(t.bid_price_1);
            bid_price_2.push(t.bid_price_2);
            bid_price_3.push(t.bid_price_3);
            bid_price_4.push(t.bid_price_4);
            bid_price_5.push(t.bid_price_5);
            ask_price_1.push(t.ask_price_1);
            ask_price_2.push(t.ask_price_2);
            ask_price_3.push(t.ask_price_3);
            ask_price_4.push(t.ask_price_4);
            ask_price_5.push(t.ask_price_5);
            bid_volume_1.push(t.bid_volume_1);
            bid_volume_2.push(t.bid_volume_2);
            bid_volume_3.push(t.bid_volume_3);
            bid_volume_4.push(t.bid_volume_4);
            bid_volume_5.push(t.bid_volume_5);
            ask_volume_1.push(t.ask_volume_1);
            ask_volume_2.push(t.ask_volume_2);
            ask_volume_3.push(t.ask_volume_3);
            ask_volume_4.push(t.ask_volume_4);
            ask_volume_5.push(t.ask_volume_5);
            open_prices.push(t.open_price);
            high_prices.push(t.high_price);
            low_prices.push(t.low_price);
            pre_closes.push(t.pre_close);
            limit_ups.push(t.limit_up);
            limit_downs.push(t.limit_down);
            gateway_name.push(t.gateway_name.clone());
        }

        DataFrame::new(vec![
            Column::new("datetime".into(), datetimes),
            Column::new("symbol".into(), symbols),
            Column::new("exchange".into(), exchanges),
            Column::new("last_price".into(), last_prices),
            Column::new("last_volume".into(), last_volumes),
            Column::new("volume".into(), volumes),
            Column::new("turnover".into(), turnovers),
            Column::new("open_interest".into(), open_interests),
            Column::new("bid_price_1".into(), bid_price_1),
            Column::new("bid_price_2".into(), bid_price_2),
            Column::new("bid_price_3".into(), bid_price_3),
            Column::new("bid_price_4".into(), bid_price_4),
            Column::new("bid_price_5".into(), bid_price_5),
            Column::new("ask_price_1".into(), ask_price_1),
            Column::new("ask_price_2".into(), ask_price_2),
            Column::new("ask_price_3".into(), ask_price_3),
            Column::new("ask_price_4".into(), ask_price_4),
            Column::new("ask_price_5".into(), ask_price_5),
            Column::new("bid_volume_1".into(), bid_volume_1),
            Column::new("bid_volume_2".into(), bid_volume_2),
            Column::new("bid_volume_3".into(), bid_volume_3),
            Column::new("bid_volume_4".into(), bid_volume_4),
            Column::new("bid_volume_5".into(), bid_volume_5),
            Column::new("ask_volume_1".into(), ask_volume_1),
            Column::new("ask_volume_2".into(), ask_volume_2),
            Column::new("ask_volume_3".into(), ask_volume_3),
            Column::new("ask_volume_4".into(), ask_volume_4),
            Column::new("ask_volume_5".into(), ask_volume_5),
            Column::new("open_price".into(), open_prices),
            Column::new("high_price".into(), high_prices),
            Column::new("low_price".into(), low_prices),
            Column::new("pre_close".into(), pre_closes),
            Column::new("limit_up".into(), limit_ups),
            Column::new("limit_down".into(), limit_downs),
            Column::new("gateway_name".into(), gateway_name),
        ])
        .map_err(|e| format!("\u{521b}\u{5efa}Tick DataFrame\u{5931}\u{8d25}: {}", e))
    }
