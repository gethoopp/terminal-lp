# Terminal LP Volatile Scanner

Rust terminal application untuk memonitor pair Solana yang volatil menggunakan DexScreener API.

## Fitur
- Refresh data otomatis setiap 15 detik
- Sorting pair berdasarkan volatilitas harga
- Filter minimum volume dan likuiditas
- Pencarian token/pair langsung di terminal
- Antarmuka TUI berbasis `ratatui`

## Menjalankan
```bash
cargo run --release
```
