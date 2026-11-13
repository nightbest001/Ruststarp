# Ruststrap

Pemasang dan peluncur Roblox untuk Windows, ditulis dengan Rust. Satu file `.exe`, tanpa .NET, tanpa terminal.
Ada jendela pengaturan dan jendela progres bertema oranye Rust, dengan tata letak mengikuti Bloxstrap.
Repo: <https://github.com/nightbest001/Ruststarp>

Ruststrap lahir dari [Bloxstrap](https://github.com/bloxstraplabs/bloxstrap) (C#, MIT). Alurnya dipindah ke
Rust, jadi bukan salinan Bloxstrap dan tidak berafiliasi dengannya.
Bukan produk resmi Roblox.

> **Status: awal.** Jendela (GUI) baru ditambahkan dan belum pernah di-compile atau dijalankan. Build lewat GitHub Actions dulu, dan siap memperbaiki error kecil.

## Kenapa ada

| | Bloxstrap | Ruststrap |
|---|---|---|
| Bahasa | C# (.NET 6, WPF) | Rust |
| Kebutuhan | .NET Desktop Runtime | tidak ada |
| Antarmuka | jendela dan tray | jendela (egui), tanpa tray |
| Pengaturan | layar settings | jendela pengaturan, atau file JSON dan perintah `flags` |
| Discord Rich Presence | ada | dihapus |
| Pilih GPU (dropdown) | tidak ada | ada |
| Setup awal (Welcome, Install, Completion) | ada | ada |
| Halaman Appearance | tema, bootstrapper, ikon | tema (sistem/terang/gelap) dan judul bootstrapper |
| Export diagnostic data | ada | ada |

## Cara pakai

Unduh `ruststrap.exe` dari halaman Releases lalu klik dua kali. Tidak ada terminal yang terbuka, yang muncul
jendela **pengaturan**. Di situ ada halaman Integrations, Behaviour, Mods, FastFlags (dan editornya), GPU,
Ruststrap (pasang, perbaiki, buka folder, hapus), dan About. Tombol **Save** menulis semua perubahan.

Klik "Play" di situs Roblox (setelah Ruststrap dipasang) membuka **jendela progres** kecil: ikon, satu baris
teks, bar, dan tombol Cancel. Jendela itu menutup sendiri begitu Roblox jalan.

Perintah terminal tetap ada untuk yang butuh. Ruststrap meminjam konsol terminal yang memanggilnya, jadi
tulisannya tetap tampil:

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

Isi `RUSTSTRAP_DEBUG=1` supaya baris log juga tampil di terminal (hanya kalau dijalankan dari terminal).

## Peta kode

```
src/main.rs              pintu masuk dan subperintah (tanpa jendela terminal)
src/ui/                  jendela: pengaturan, progres, tema oranye, komponen
src/progress.rs          status unduhan yang dibaca jendela progres
src/console.rs           meminjam terminal pemanggil untuk subperintah
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

Ikon di tray, gaya bootstrapper dan ikon kustom, bahasa, analytics, extract ikon, update otomatis, preset mod
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
