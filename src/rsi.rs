/// RSI per emiten/pair — dihitung dari perubahan harga multi-timeframe.
///
/// Karena API DexScreener tidak menyediakan data OHLCV mentah, RSI dihitung
/// secara heuristik menggunakan delta antar-window dari data price_change yang
/// tersedia: h24-h6, h6-h1, h1-m5. Ini bukan RSI 14-candle penuh, tetapi
/// setidaknya menangkap perubahan momentum antar-periode secara konseptual.
use crate::api::PriceChange;

/// Zona RSI dan interpretasinya untuk LP entry.
#[derive(Debug, Clone, PartialEq)]
pub enum RsiZone {
    /// RSI < 30 — oversold, potensi entry beli / LP bullish
    Oversold,
    /// RSI 30–45 — mulai rebound dari oversold, perhatikan konfirmasi
    RecoveringOversold,
    /// RSI 45–55 — netral, arah belum jelas
    Neutral,
    /// RSI 55–70 — momentum naik, normal untuk entry LP
    Bullish,
    /// RSI > 70 — overbought, risiko reversal / entry LP berisiko
    Overbought,
}

impl RsiZone {
    pub fn label(&self) -> &'static str {
        match self {
            RsiZone::Oversold => "Oversold",
            RsiZone::RecoveringOversold => "Recovering",
            RsiZone::Neutral => "Netral",
            RsiZone::Bullish => "Bullish",
            RsiZone::Overbought => "Overbought",
        }
    }

    pub fn entry_bias(&self) -> &'static str {
        match self {
            RsiZone::Oversold => {
                "Potensi mean reversion menarik, tapi tunggu tekanan jual benar-benar reda"
            }
            RsiZone::RecoveringOversold => {
                "Rebound awal mulai terlihat; LP kecil bertahap lebih aman daripada entry penuh"
            }
            RsiZone::Neutral => {
                "Kondisi paling nyaman untuk LP karena arah lebih seimbang dan IL cenderung lebih rendah"
            }
            RsiZone::Bullish => {
                "Trend naik masih sehat, tetapi LP tetap perlu disiplin range dan sizing"
            }
            RsiZone::Overbought => {
                "Harga jauh dari area mean; LP baru rawan masuk dekat puncak lalu kena IL saat revert"
            }
        }
    }
}

/// Hasil kalkulasi RSI untuk satu pair.
#[derive(Debug, Clone)]
pub struct RsiInfo {
    pub value: f64,
    pub zone: RsiZone,
}

impl RsiInfo {
    /// Hitung RSI dari data price_change yang tersedia.
    ///
    /// Menggunakan delta antar window kumulatif: (h24-h6), (h6-h1), (h1-m5).
    /// Dengan demikian input RSI lebih dekat ke pseudo-return berurutan daripada
    /// memakai cumulative change mentah yang menyesatkan.
    pub fn from_price_change(pc: &PriceChange) -> Option<Self> {
        let periods: Vec<f64> = [pc.h24, pc.h6, pc.h1, pc.m5]
            .iter()
            .filter_map(|v| *v)
            .collect();

        if periods.len() < 3 {
            return None;
        }

        let deltas: Vec<f64> = periods
            .windows(2)
            .map(|window| window[0] - window[1])
            .collect();

        if deltas.len() < 2 {
            return None;
        }

        let value = compute_rsi(&deltas);
        let zone = classify_rsi(value);
        Some(RsiInfo { value, zone })
    }

    /// Formatted label untuk ditampilkan di UI.
    pub fn display(&self) -> String {
        format!(
            "{:.1} ({}) — {}",
            self.value,
            self.zone.label(),
            self.zone.entry_bias()
        )
    }

    /// Singkat: nilai + zona — digunakan untuk kolom RSI di tabel screener.
    #[allow(dead_code)]
    pub fn short_display(&self) -> String {
        format!("{:.1} ({})", self.value, self.zone.label())
    }
}

/// Hitung RSI dari slice perubahan harga (gain/loss).
/// Dengan data tipis, hasil dipaksa netral untuk menghindari sinyal palsu.
pub fn compute_rsi(changes: &[f64]) -> f64 {
    if changes.len() < 2 {
        return 50.0;
    }

    let gains: Vec<f64> = changes
        .iter()
        .map(|&c| if c > 0.0 { c } else { 0.0 })
        .collect();
    let losses: Vec<f64> = changes
        .iter()
        .map(|&c| if c < 0.0 { -c } else { 0.0 })
        .collect();

    let avg_gain: f64 = gains.iter().sum::<f64>() / gains.len() as f64;
    let avg_loss: f64 = losses.iter().sum::<f64>() / losses.len() as f64;

    if avg_gain == 0.0 && avg_loss == 0.0 {
        return 50.0;
    }
    if avg_loss == 0.0 {
        return 100.0;
    }
    if avg_gain == 0.0 {
        return 0.0;
    }

    let rs = avg_gain / avg_loss;
    (100.0 - (100.0 / (1.0 + rs))).clamp(0.0, 100.0)
}

pub fn classify_rsi(value: f64) -> RsiZone {
    match value {
        v if v < 30.0 => RsiZone::Oversold,
        v if v < 45.0 => RsiZone::RecoveringOversold,
        v if v < 55.0 => RsiZone::Neutral,
        v if v < 70.0 => RsiZone::Bullish,
        _ => RsiZone::Overbought,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pc(m5: Option<f64>, h1: Option<f64>, h6: Option<f64>, h24: Option<f64>) -> PriceChange {
        PriceChange { m5, h1, h6, h24 }
    }

    #[test]
    fn test_rsi_all_gains_is_100() {
        let rsi = compute_rsi(&[5.0, 10.0, 3.0]);
        assert_eq!(rsi, 100.0);
    }

    #[test]
    fn test_rsi_all_losses_is_0() {
        let rsi = compute_rsi(&[-5.0, -10.0, -3.0]);
        assert_eq!(rsi, 0.0);
    }

    #[test]
    fn test_rsi_equal_gain_loss_is_50() {
        let rsi = compute_rsi(&[5.0, -5.0]);
        assert!((rsi - 50.0).abs() < 1e-6, "expected ~50.0, got {rsi}");
    }

    #[test]
    fn test_rsi_single_positive_returns_neutral_when_data_too_thin() {
        assert_eq!(compute_rsi(&[3.0]), 50.0);
    }

    #[test]
    fn test_rsi_single_negative_returns_neutral_when_data_too_thin() {
        assert_eq!(compute_rsi(&[-3.0]), 50.0);
    }

    #[test]
    fn test_rsi_empty_returns_neutral() {
        assert_eq!(compute_rsi(&[]), 50.0);
    }

    #[test]
    fn test_rsi_all_zero_returns_neutral() {
        assert_eq!(compute_rsi(&[0.0, 0.0, 0.0]), 50.0);
    }

    #[test]
    fn test_rsi_clamp_within_0_100() {
        let rsi = compute_rsi(&[100.0, 100.0, 100.0]);
        assert!((0.0..=100.0).contains(&rsi));
    }

    #[test]
    fn test_classify_oversold() {
        assert_eq!(classify_rsi(20.0), RsiZone::Oversold);
        assert_eq!(classify_rsi(0.0), RsiZone::Oversold);
    }

    #[test]
    fn test_classify_recovering() {
        assert_eq!(classify_rsi(35.0), RsiZone::RecoveringOversold);
    }

    #[test]
    fn test_classify_neutral() {
        assert_eq!(classify_rsi(50.0), RsiZone::Neutral);
    }

    #[test]
    fn test_classify_bullish() {
        assert_eq!(classify_rsi(60.0), RsiZone::Bullish);
    }

    #[test]
    fn test_classify_overbought() {
        assert_eq!(classify_rsi(75.0), RsiZone::Overbought);
        assert_eq!(classify_rsi(100.0), RsiZone::Overbought);
    }

    #[test]
    fn test_from_price_change_full_data() {
        let rsi = RsiInfo::from_price_change(&pc(Some(2.0), Some(5.0), Some(8.0), Some(10.0)));
        let info = rsi.expect("should produce RSI");
        assert_eq!(info.value, 100.0);
        assert_eq!(info.zone, RsiZone::Overbought);
    }

    #[test]
    fn test_from_price_change_all_negative() {
        let rsi = RsiInfo::from_price_change(&pc(Some(-2.0), Some(-5.0), Some(-8.0), Some(-10.0)));
        let info = rsi.expect("should produce RSI");
        assert_eq!(info.value, 0.0);
        assert_eq!(info.zone, RsiZone::Oversold);
    }

    #[test]
    fn test_from_price_change_missing_data_returns_none() {
        let rsi = RsiInfo::from_price_change(&pc(None, None, None, Some(5.0)));
        assert!(rsi.is_none());
    }

    #[test]
    fn test_from_price_change_partial_data() {
        let rsi = RsiInfo::from_price_change(&pc(None, Some(1.0), Some(4.0), Some(7.0)));
        let info = rsi.expect("should produce RSI");
        assert_eq!(info.value, 100.0);
        assert_eq!(info.zone, RsiZone::Overbought);
    }

    #[test]
    fn test_from_price_change_uses_delta_not_raw_cumulative_values() {
        let rsi = RsiInfo::from_price_change(&pc(Some(25.0), Some(24.0), Some(22.0), Some(20.0)));
        let info = rsi.expect("should produce RSI");
        assert_eq!(info.value, 0.0);
        assert_eq!(info.zone, RsiZone::Oversold);
    }

    #[test]
    fn test_display_contains_value_and_bias() {
        let info = RsiInfo {
            value: 72.5,
            zone: RsiZone::Overbought,
        };
        let d = info.display();
        assert!(d.contains("72.5"));
        assert!(d.contains("Overbought"));
    }

    #[test]
    fn test_short_display_format() {
        let info = RsiInfo {
            value: 50.0,
            zone: RsiZone::Neutral,
        };
        assert_eq!(info.short_display(), "50.0 (Netral)");
    }

    #[test]
    fn test_entry_bias_oversold_suggests_mean_reversion() {
        let bias = RsiZone::Oversold.entry_bias();
        assert!(bias.to_lowercase().contains("mean") || bias.to_lowercase().contains("jual"));
    }

    #[test]
    fn test_entry_bias_overbought_mentions_il_or_revert() {
        let bias = RsiZone::Overbought.entry_bias();
        assert!(
            bias.to_lowercase().contains("il")
                || bias.to_lowercase().contains("mean")
                || bias.to_lowercase().contains("revert")
        );
    }

    #[test]
    fn test_entry_bias_neutral_is_lp_friendly() {
        let bias = RsiZone::Neutral.entry_bias();
        assert!(bias.to_lowercase().contains("lp") || bias.to_lowercase().contains("il"));
    }
}
