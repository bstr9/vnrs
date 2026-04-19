
    // ----- DataFrame converters -----

    fn bars_to_dataframe(bars: &[BarData]) -> Result<DataFrame, String> {
        let n = bars.len();
        let mut datetimes: Vec<i64> = Vec::with_capacity(n);
        let mut symbols: Vec<String> = Vec::with_capacity(n);
        let mut exchanges: Vec<String> = Vec::with_capacity(n);
        let mut intervals: Vec<String> = Vec::with_capacity(n);
        let mut open_prices: Vec<f64> = Vec::with_capacity(n);
        let mut high_prices: Vec<f64> = Vec::with_capacity(n);
        let mut low_prices: Vec<f64> = Vec::with_capacity(n);
        let mut close_prices: Vec<f64> = Vec::with_capacity(n);
        let mut volumes: Vec<f64> = Vec::with_capacity(n);
        let mut turnovers: Vec<f64> = Vec::with_capacity(n);
        let mut open_interests: Vec<f64> = Vec::with_capacity(n);
        let mut gateway_name: Vec<String> = Vec::with_capacity(n);

        for bar in bars {
            datetimes.push(bar.datetime.timestamp_millis());
            symbols.push(bar.symbol.clone());
            exchanges.push(bar.exchange.value().to_string());
            intervals.push(bar.interval.map(|i| i.value().to_string()).unwrap_or_default());
            open_prices.push(bar.open_price);
            high_prices.push(bar.high_price);
            low_prices.push(bar.low_price);
            close_prices.push(bar.close_price);
            volumes.push(bar.volume);
            turnovers.push(bar.turnover);
            open_interests.push(bar.open_interest);
            gateway_name.push(bar.gateway_name.clone());
        }

        DataFrame::new(vec![
            Column::new("datetime".into(), datetimes),
            Column::new("symbol".into(), symbols),
            Column::new("exchange".into(), exchanges),
            Column::new("interval".into(), intervals),
            Column::new("open_price".into(), open_prices),
            Column::new("high_price".into(), high_prices),
            Column::new("low_price".into(), low_prices),
            Column::new("close_price".into(), close_prices),
            Column::new("volume".into(), volumes),
            Column::new("turnover".into(), turnovers),
            Column::new("open_interest".into(), open_interests),
            Column::new("gateway_name".into(), gateway_name),
        ])
        .map_err(|e| format!("\u{521b}\u{5efa}Bar DataFrame\u{5931}\u{8d25}: {}", e))
    }

    fn dataframe_to_bars(df: &DataFrame) -> Result<Vec<BarData>, String> {
        let height = df.height();
        if height == 0 { return Ok(Vec::new()); }

        let datetimes = df.column("datetime").map_err(|e| format!("{}", e))?.i64().map_err(|e| format!("{}", e))?;
        let symbols = df.column("symbol").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let exchanges = df.column("exchange").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let intervals = df.column("interval").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let open_prices = df.column("open_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let high_prices = df.column("high_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let low_prices = df.column("low_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let close_prices = df.column("close_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let volumes = df.column("volume").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let turnovers = df.column("turnover").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let open_interests = df.column("open_interest").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let gateway_names = df.column("gateway_name").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;

        let mut bars = Vec::with_capacity(height);
        for i in 0..height {
            let dt_millis = datetimes.get(i).unwrap_or(0);
            let datetime = DateTime::from_timestamp_millis(dt_millis).unwrap_or_else(Utc::now);
            let exchange_str = exchanges.get(i).unwrap_or("BINANCE");
            let interval_str = intervals.get(i).unwrap_or("");

            bars.push(BarData {
                gateway_name: gateway_names.get(i).unwrap_or("").to_string(),
                symbol: symbols.get(i).unwrap_or("").to_string(),
                exchange: exchange_from_str(exchange_str).unwrap_or(Exchange::Binance),
                datetime,
                interval: if interval_str.is_empty() { None } else { interval_from_str(interval_str) },
                volume: volumes.get(i).unwrap_or(0.0),
                turnover: turnovers.get(i).unwrap_or(0.0),
                open_interest: open_interests.get(i).unwrap_or(0.0),
                open_price: open_prices.get(i).unwrap_or(0.0),
                high_price: high_prices.get(i).unwrap_or(0.0),
                low_price: low_prices.get(i).unwrap_or(0.0),
                close_price: close_prices.get(i).unwrap_or(0.0),
                extra: None,
            });
        }
        Ok(bars)
    }
