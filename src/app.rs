use crate::api::PairData;
use chrono::Local;

#[derive(Debug, Clone, PartialEq)]
pub enum SortColumn {
    Symbol,
    Price,
    Change24h,
    Change1h,
    Volume24h,
    Liquidity,
    FDV,
    Age,
}

impl SortColumn {
    pub fn next(&self) -> Self {
        match self {
            SortColumn::Symbol => SortColumn::Price,
            SortColumn::Price => SortColumn::Change24h,
            SortColumn::Change24h => SortColumn::Change1h,
            SortColumn::Change1h => SortColumn::Volume24h,
            SortColumn::Volume24h => SortColumn::Liquidity,
            SortColumn::Liquidity => SortColumn::FDV,
            SortColumn::FDV => SortColumn::Age,
            SortColumn::Age => SortColumn::Symbol,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SortColumn::Symbol => SortColumn::Age,
            SortColumn::Price => SortColumn::Symbol,
            SortColumn::Change24h => SortColumn::Price,
            SortColumn::Change1h => SortColumn::Change24h,
            SortColumn::Volume24h => SortColumn::Change1h,
            SortColumn::Liquidity => SortColumn::Volume24h,
            SortColumn::FDV => SortColumn::Liquidity,
            SortColumn::Age => SortColumn::FDV,
        }
    }

}

#[derive(Debug, Clone, PartialEq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(&self) -> Self {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }
}

pub struct App {
    pub pairs: Vec<PairData>,
    pub filtered_pairs: Vec<usize>,
    pub sort_column: SortColumn,
    pub sort_direction: SortDirection,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub status_message: String,
    pub last_update: String,
    pub min_volume_filter: f64,
    pub min_liquidity_filter: f64,
    pub search_query: String,
    pub is_loading: bool,
    pub show_help: bool,
    pub error_message: Option<String>,
}

impl App {
    pub fn new() -> Self {
        App {
            pairs: Vec::new(),
            filtered_pairs: Vec::new(),
            sort_column: SortColumn::Change24h,
            sort_direction: SortDirection::Descending,
            selected_index: 0,
            scroll_offset: 0,
            status_message: String::from("Initializing..."),
            last_update: String::from("--"),
            min_volume_filter: 1000.0,
            min_liquidity_filter: 500.0,
            search_query: String::new(),
            is_loading: true,
            show_help: false,
            error_message: None,
        }
    }

    pub fn update_pairs(&mut self, pairs: Vec<PairData>) {
        self.pairs = pairs;
        self.apply_filters();
        self.last_update = Local::now().format("%H:%M:%S").to_string();
        self.status_message = format!("Loaded {} pairs", self.pairs.len());
        self.is_loading = false;
        self.error_message = None;

        if self.selected_index >= self.filtered_pairs.len() {
            self.selected_index = self.filtered_pairs.len().saturating_sub(1);
        }
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_message = Some(msg.clone());
        self.status_message = msg;
        self.is_loading = false;
    }

    pub fn apply_filters(&mut self) {
        let query = self.search_query.to_lowercase();
        let mut indices: Vec<usize> = self
            .pairs
            .iter()
            .enumerate()
            .filter(|(_, pair)| {
                let vol = pair.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                let liq = pair.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);

                if vol < self.min_volume_filter || liq < self.min_liquidity_filter {
                    return false;
                }

                if !query.is_empty() {
                    let symbol = pair
                        .base_token
                        .symbol
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase();
                    let name = pair
                        .base_token
                        .name
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase();
                    if !symbol.contains(&query) && !name.contains(&query) {
                        return false;
                    }
                }

                true
            })
            .map(|(i, _)| i)
            .collect();

        self.sort_indices(&mut indices);
        self.filtered_pairs = indices;
        self.status_message = format!(
            "Showing {} / {} pairs | Vol >= ${:.0} | Liq >= ${:.0}",
            self.filtered_pairs.len(),
            self.pairs.len(),
            self.min_volume_filter,
            self.min_liquidity_filter
        );
    }

    fn sort_indices(&self, indices: &mut [usize]) {
        let pairs = &self.pairs;
        let desc = self.sort_direction == SortDirection::Descending;

        indices.sort_by(|&a, &b| {
            let pa = &pairs[a];
            let pb = &pairs[b];
            let cmp = match self.sort_column {
                SortColumn::Symbol => {
                    let sym_a = pa.base_token.symbol.as_deref().unwrap_or("");
                    let sym_b = pb.base_token.symbol.as_deref().unwrap_or("");
                    sym_a.cmp(sym_b)
                }
                SortColumn::Price => {
                    let price_a = parse_usd(&pa.price_usd);
                    let price_b = parse_usd(&pb.price_usd);
                    price_a.partial_cmp(&price_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Change24h => {
                    let ch_a = pa.price_change.as_ref().and_then(|pc| pc.h24).unwrap_or(0.0);
                    let ch_b = pb.price_change.as_ref().and_then(|pc| pc.h24).unwrap_or(0.0);
                    ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Change1h => {
                    let ch_a = pa.price_change.as_ref().and_then(|pc| pc.h1).unwrap_or(0.0);
                    let ch_b = pb.price_change.as_ref().and_then(|pc| pc.h1).unwrap_or(0.0);
                    ch_a.partial_cmp(&ch_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Volume24h => {
                    let vol_a = pa.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                    let vol_b = pb.volume.as_ref().and_then(|v| v.h24).unwrap_or(0.0);
                    vol_a.partial_cmp(&vol_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Liquidity => {
                    let liq_a = pa.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                    let liq_b = pb.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0);
                    liq_a.partial_cmp(&liq_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::FDV => {
                    let fdv_a = pa.fdv.unwrap_or(0.0);
                    let fdv_b = pb.fdv.unwrap_or(0.0);
                    fdv_a.partial_cmp(&fdv_b).unwrap_or(std::cmp::Ordering::Equal)
                }
                SortColumn::Age => {
                    let age_a = pa.pair_created_at.unwrap_or(0);
                    let age_b = pb.pair_created_at.unwrap_or(0);
                    age_a.cmp(&age_b)
                }
            };
            if desc {
                cmp.reverse()
            } else {
                cmp
            }
        });
    }

    pub fn selected_pair(&self) -> Option<&PairData> {
        self.filtered_pairs
            .get(self.selected_index)
            .map(|&i| &self.pairs[i])
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            if self.selected_index < self.scroll_offset {
                self.scroll_offset = self.selected_index;
            }
        }
    }

    pub fn move_selection_down(&mut self, visible_rows: usize) {
        if self.selected_index + 1 < self.filtered_pairs.len() {
            self.selected_index += 1;
            if self.selected_index >= self.scroll_offset + visible_rows {
                self.scroll_offset = self.selected_index.saturating_sub(visible_rows.saturating_sub(1));
            }
        }
    }

    pub fn toggle_sort_direction(&mut self) {
        self.sort_direction = self.sort_direction.toggle();
        self.apply_filters();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

fn parse_usd(s: &Option<String>) -> f64 {
    s.as_ref()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}
