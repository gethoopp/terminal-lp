# Terminal LP Enhancement - Design

## Architecture Changes

## 1. `config.rs`
- Menambahkan modul konfigurasi terpusat dengan struktur `ScannerConfig`.
- Menggunakan `serde` + `toml` untuk deserialize/serialize.
- `OnceLock` dipakai untuk menyimpan active config global agar bisa diakses lintas modul.
- Menyediakan helper path:
  - `config_file_path()`
  - `app_data_dir()`
  - `watchlist_file_path()`
  - `bot_position_file_path()`

## 2. `logger.rs`
- Inisialisasi logging file memakai `tracing` + `tracing-subscriber` + `tracing-appender`.
- Format log non-ANSI untuk file readability.
- Level log berasal dari config.

## 3. `api.rs`
- Hilangkan endpoint hardcode DexScreener; baca dari config.
- Query list, retry count, delay, dan top-N limit menjadi configurable.
- Tambah flow fallback:
  - Jika hasil DexScreener kosong/gagal, panggil Jupiter quote endpoint.
  - Parse response quote untuk membentuk synthetic `PairData` minimal.
  - Tambahkan warning agar UI tahu data berasal dari fallback.

## 4. `app.rs`
- Inisialisasi filter default dari config.
- Ubah persistence watchlist & bot-position dari HOME dotfile ke data-dir scanner.
- Tambahkan:
  - `collect_price_alerts()` dengan cooldown per pair
  - `export_filtered_to_csv()` untuk output pair + assessment
- Backward compatibility read: jika file baru belum ada, bisa read file legacy lama.

## 5. `main.rs`
- Startup flow:
  - init config
  - init logger
  - init terminal UI
- Interval tick/refresh/bot check ambil dari config.
- Hotkey baru: `x` untuk export CSV.
- Saat fetch result masuk:
  - jalankan deteksi alert
  - emit terminal bell (opsional)
  - tulis log event penting.
- Konfigurasi bot tidak lagi dibaca dari env/hardcoded, tetapi dari config file.

## 6. `ui.rs`
- Perbarui hint control untuk menampilkan hotkey export.
- Update help text supaya menunjuk config/log path baru.
- Loading text menyebut fallback Jupiter.

## Data Flow Summary
1. `main` load config -> `OnceLock`
2. `main` init logger
3. `api` baca config untuk fetch DexScreener
4. jika gagal -> `api` fetch Jupiter quote fallback
5. `main` kirim data ke `app`
6. `app` update pairs + alert detection + optional export via hotkey
7. event penting dicatat ke log file

## Trade-offs
- Fallback Jupiter quote tidak setara richness DexScreener (data lebih minimal).
- Global config via `OnceLock` menyederhanakan wiring, tapi runtime hot-reload belum didukung.
