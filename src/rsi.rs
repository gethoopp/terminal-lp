/// RSI per emiten/pair — dihitung dari perubahan harga multi-timeframe.
///
/// Karena API DexScreener tidak menyediakan data OHLCV mentah, RSI dihitung
/// secara heuristik menggunakan "pseudo-periods" dari data price_change yang
/// tersedia: m5, h1, h6, h24. Setiap nilai dianggap sebagai perubahan pada
/// periode tersebut dan membentuk urutan periode singkat yang membentuk RSI.
///
/// Urutan periode (dari lama ke baru): h24, h6, h1, m5
/// Nilai positif = gain, negatif = loss.
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
            RsiZone::Oversold => "Entry contrarian bisa dipertimbangkan; konfirmasi reversal dulu",
            RsiZone::RecoveringOversold => "Awal rebound — entry kecil bisa, sizing konservatif",
            RsiZone::Neutral => "Arah belum jelas, tunggu breakout momentum",
            RsiZone::Bullish => "Momentum positif, entry LP masih layak",
            RsiZone::Overbought => "Overbought — hindari entry baru, tunggu koreksi",
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
    /// Menggunakan 4 "period" dari urutan h24 → h6 → h1 → m5,
    /// di mana setiap periode merepresentasikan perubahan harga pada rentang waktu itu.
    /// Karena hanya 4 titik, RSI yang dihasilkan adalah estimasi sinyal momentum jangka pendek.
    pub fn from_price_change(pc: &PriceChange) -> Option<Self> {
        // Kumpulkan perubahan dari periode terpanjang ke terpendek (urutan kronologis)
        let periods: Vec<f64> = [pc.h24, pc.h6, pc.h1, pc.m5]
            .iter()
            .filter_map(|v| *v)
            .collect();

        if periods.len() < 2 {
            return None;
        }

        let value = compute_rsi(&periods);
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
/// Menggunakan metode Wilder's Smoothed Moving Average (SMMA).
/// Minimal 2 periode diperlukan.
pub fn compute_rsi(changes: &[f64]) -> f64 {
    if changes.is_empty() {
        return 50.0;
    }
    if changes.len() == 1 {
        return if changes[0] >= 0.0 { 70.0 } else { 30.0 };
    }

    let gains: Vec<f64> = changes
        .iter()
        .map(|&c| if c > 0.0 { c } else { 0.0 })
        .collect();
    let losses: Vec<f64> = changes
        .iter()
        .map(|&c| if c < 0.0 { -c } else { 0.0 })
        .collect();

    // Dengan data sangat sedikit, pakai average sederhana (ekuivalen dengan Wilder periode=n)
    let avg_gain: f64 = gains.iter().sum::<f64>() / gains.len() as f64;
    let avg_loss: f64 = losses.iter().sum::<f64>() / losses.len() as f64;

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

    // --- compute_rsi ---

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
        // avg_gain == avg_loss => RS=1 => RSI = 100 - 100/(1+1) = 50
        let rsi = compute_rsi(&[5.0, -5.0]);
        assert!((rsi - 50.0).abs() < 1e-6, "expected ~50.0, got {rsi}");
    }

    #[test]
    fn test_rsi_single_positive() {
        assert_eq!(compute_rsi(&[3.0]), 70.0);
    }

    #[test]
    fn test_rsi_single_negative() {
        assert_eq!(compute_rsi(&[-3.0]), 30.0);
    }

    #[test]
    fn test_rsi_empty_returns_neutral() {
        assert_eq!(compute_rsi(&[]), 50.0);
    }

    #[test]
    fn test_rsi_clamp_within_0_100() {
        let rsi = compute_rsi(&[100.0, 100.0, 100.0]);
        assert!((0.0..=100.0).contains(&rsi));
    }

    // --- classify_rsi ---

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

    // --- RsiInfo::from_price_change ---

    #[test]
    fn test_from_price_change_full_data() {
        // All positive => overbought
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
        // Only 1 data point
        let rsi = RsiInfo::from_price_change(&pc(None, None, None, Some(5.0)));
        assert!(rsi.is_none());
    }

    #[test]
    fn test_from_price_change_partial_data() {
        // m5=None, h1=None, h6=-5.0, h24=5.0 => 2 periods, equal gain/loss => ~50
        let rsi = RsiInfo::from_price_change(&pc(None, None, Some(-5.0), Some(5.0)));
        let info = rsi.expect("should produce RSI");
        assert!(
            (info.value - 50.0).abs() < 1e-6,
            "expected ~50, got {}",
            info.value
        );
        assert_eq!(info.zone, RsiZone::Neutral);
    }

    #[test]
    fn test_from_price_change_mixed_produces_correct_zone() {
        // Predominantly positive (3 gain, 1 loss)
        let rsi = RsiInfo::from_price_change(&pc(Some(1.0), Some(3.0), Some(2.0), Some(-5.0)));
        let info = rsi.expect("should produce RSI");
        // avg_gain = (1+3+2)/4 = 1.5, avg_loss = 5/4 = 1.25, RS=1.2 => RSI~54.5
        assert!(info.value > 50.0 && info.value < 70.0);
        assert!(matches!(info.zone, RsiZone::Neutral | RsiZone::Bullish));
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

    // --- entry_bias correctness ---

    #[test]
    fn test_entry_bias_oversold_suggests_contrarian() {
        let bias = RsiZone::Oversold.entry_bias();
        assert!(
            bias.to_lowercase().contains("contrarian")
                || bias.to_lowercase().contains("konfirmasi")
        );
    }

    #[test]
    fn test_entry_bias_overbought_suggests_avoid() {
        let bias = RsiZone::Overbought.entry_bias();
        assert!(bias.to_lowercase().contains("hindari") || bias.to_lowercase().contains("koreksi"));
    }
}
