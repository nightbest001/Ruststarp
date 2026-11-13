# Ruststrap

Pemasang dan peluncur Roblox untuk Windows, ditulis dengan Rust. Satu file `.exe`, tanpa .NET, tanpa jendela.
Repo: <https://github.com/nightbest001/Ruststarp>

Ruststrap lahir dari [Bloxstrap](https://github.com/bloxstraplabs/bloxstrap) (C#, MIT). Alurnya dipindah ke
Rust dan dibuat berbasis perintah (CLI), jadi bukan salinan Bloxstrap dan tidak berafiliasi dengannya.
Bukan produk resmi Roblox.

> **Status: awal (v0.1).** Kode belum pernah di-compile. Build sendiri dulu dan siap memperbaiki error kecil.

## Kenapa ada

| | Bloxstrap | Ruststrap |
|---|---|---|
| Bahasa | C# (.NET 6, WPF) | Rust |
| Kebutuhan | .NET Desktop Runtime | tidak ada |
| Antarmuka | jendela dan tray | perintah di terminal |
| Pengaturan | layar settings | file JSON dan perintah `flags` |
| Discord Rich Presence | ada | belum |

## Cara pakai

Unduh `ruststrap.exe` dari halaman Releases, lalu di terminal:

```
ruststrap install                 pasang dan daftarkan link roblox://
ruststrap -player                 update lalu jalankan Roblox
ruststrap -player -nolaunch       update saja
ruststrap -studio                 jalankan Roblox Studio
ruststrap -player -channel <nama> pakai channel tertentu
ruststrap -player -force          pasang ulang
ruststrap uninstall               hapus semuanya
```

Setelah `install`, klik "Play" di situs Roblox akan membuka game lewat Ruststrap.

### FastFlags

```
ruststrap flags list
ruststrap flags set DFIntTaskSchedulerTargetFps 144
ruststrap flags remove DFIntTaskSchedulerTargetFps
ruststrap flags preset msaa 4
ruststrap flags preset texture 1
ruststrap flags preset disable-scaling on
```

Flag dipakai saat Roblox dijalankan lewat Ruststrap berikutnya.

### Modifikasi file

Taruh file di `%LOCALAPPDATA%\Ruststrap\Modifications\`, dengan susunan folder yang sama seperti folder Roblox
(contoh: `content\sounds\ouch.ogg`). File akan disalin ke versi Roblox saat peluncuran. Kalau kamu hapus file itu,
file asli dikembalikan.

### Lokasi data

| Apa | Di mana |
|---|---|
| Pengaturan | `%LOCALAPPDATA%\Ruststrap\Settings.json` |
| Modifikasi dan FastFlags | `%LOCALAPPDATA%\Ruststrap\Modifications\` |
| Versi Roblox | `%LOCALAPPDATA%\Ruststrap\Versions\` |
| Log | `%LOCALAPPDATA%\Ruststrap\Logs\` |

Isi `RUSTSTRAP_DEBUG=1` supaya baris log juga tampil di terminal.

## Peta kode

```
src/main.rs              pintu masuk dan subperintah
src/bootstrapper.rs      cek versi, unduh, verifikasi MD5, ekstrak, mod, jalankan
src/deployment.rs        balapan mirror CDN, channel, info versi
src/launch_settings.rs   baca argumen dan URI roblox:
src/fast_flags.rs        FastFlags dan preset
src/installer.rs         pasang dan hapus
src/registry.rs          protokol roblox:// dan entri uninstall
src/watcher.rs           proses latar yang menunggu Roblox tutup
src/activity_watcher.rs  baca log Roblox: masuk dan keluar game
src/models.rs            Settings, State, manifest paket
```

## Yang belum ada

Jendela pengaturan, Discord Rich Presence, tema bootstrapper, update otomatis, shortcut, preset mod
(emoji, kursor, font), updater latar, dan pemasangan WebView2 otomatis.

## Build

```
cargo build --release
```

Hasil: `target\release\ruststrap.exe`. Hanya untuk Windows. Tag `vX.Y.Z` memicu build dan rilis otomatis lewat GitHub Actions.

## GPU

Ruststrap baca GPU dari registry dan bisa paksa Roblox pakai GPU cepat (berguna di laptop dengan dua GPU).

```
ruststrap gpu                 # daftar GPU + pilihan sekarang
ruststrap gpu set high        # Roblox pakai GPU performa tinggi
ruststrap gpu set saving      # Roblox pakai GPU hemat daya
ruststrap gpu set default     # lepas, biar Windows yang pilih
```

Pilihan ditulis ke `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` untuk `RobloxPlayerBeta.exe` dan `RobloxStudioBeta.exe`, dan ditulis ulang tiap Roblox dijalankan (folder versi baru tiap update). Jenis GPU (discrete/integrated) hasil tebakan dari nama dan VRAM.
