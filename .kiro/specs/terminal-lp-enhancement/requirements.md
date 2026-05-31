# Terminal LP Enhancement - Requirements

## Context
Scanner saat ini stabil untuk personal use, tetapi belum siap untuk operasi production-like (konfigurasi manual, observability minim, single data source, tanpa export, tanpa alert).

## Goals (Fase 1)
1. Menstandarkan konfigurasi runtime melalui file TOML.
2. Menambahkan logging persisten untuk troubleshooting.
3. Menyediakan fallback data source Jupiter saat DexScreener gagal.
4. Menyediakan export CSV dari hasil screening.
5. Menambahkan notifikasi alert harga berbasis threshold.

## Functional Requirements

### R1. Config File System
- Aplikasi harus membaca konfigurasi dari `~/.config/haliim-scanner/config.toml`.
- Jika file tidak ada, aplikasi harus membuat file default otomatis.
- Konfigurasi minimal mencakup:
  - API (DexScreener + Jupiter fallback)
  - App refresh interval dan filter default
  - Bot endpoint/auth
  - Alert threshold
  - Export directory
  - Logging level + output directory

### R2. Logging System
- Aplikasi harus menulis log ke `~/.local/share/haliim-scanner/scanner.log`.
- Log minimal mencakup event startup/shutdown, fetch warning/error, bot error, alert trigger, export result.
- Log level harus bisa dikontrol dari config.

### R3. Jupiter API Fallback
- Saat hasil DexScreener kosong/failed, aplikasi harus mencoba fallback ke endpoint Jupiter quote.
- Endpoint fallback harus configurable dan default mencakup legacy `quote-api.jup.ag/v6/quote`.
- Jika fallback sukses, aplikasi tetap berjalan dan memberi warning status bahwa data berasal dari fallback.

### R4. Export to CSV
- User dapat mengekspor data pair + assessment via hotkey (`x`).
- File CSV disimpan ke direktori export dari config.
- Nama file harus menyertakan timestamp agar tidak overwrite.

### R5. Price Alerts
- Alert berjalan otomatis setiap update data.
- Trigger saat perubahan harga 24h pair melewati threshold konfigurasi.
- Harus ada cooldown per pair untuk mencegah spam alert.
- Notifikasi ditampilkan di terminal status dan optional terminal bell.

## Non-Functional Requirements
- Tetap kompatibel dengan unit test existing.
- Tidak mengubah behavior inti analisa heuristik.
- Graceful fallback: kegagalan fitur baru tidak boleh crash app.
