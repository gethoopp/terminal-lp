# Terminal LP Enhancement - Tasks

## Phase 1 Implementation Tasks

1. Config Foundation
- [x] Tambah dependency `toml`, `dirs`, `tracing*`, `csv`.
- [x] Buat `src/config.rs` + schema config lengkap.
- [x] Implement auto-create default `config.toml`.

2. Logging
- [x] Buat `src/logger.rs`.
- [x] Inisialisasi logger pada startup.
- [x] Tambah log event fetch/error/alert/export.

3. API Hardening + Fallback
- [x] Migrasi endpoint/query/retry dari hardcode ke config.
- [x] Implement Jupiter quote fallback saat DexScreener kosong/gagal.
- [x] Surface warning fallback ke UI status.

4. App State + Persistence
- [x] Migrasi path watchlist/bot-position ke `~/.local/share/haliim-scanner/`.
- [x] Tambah backward-read file legacy.
- [x] Tambah engine price alert + cooldown per pair.

5. CSV Export
- [x] Tambah method export di `App`.
- [x] Integrasi hotkey `x` di event loop.
- [x] Tambah feedback status sukses/gagal export.

6. UI/Help Update
- [x] Update hint hotkey + help text config/log path.
- [x] Update loading message untuk fallback.

7. Verification
- [ ] Jalankan `cargo test` dan benahi regression jika ada.
- [ ] Manual run scanner untuk verifikasi hotkey `x` dan alert behavior.
