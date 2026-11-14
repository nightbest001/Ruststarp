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
| Antarmuka | jendela dan tray | jendela (WebView2, HTML/CSS/JS) dan ikon System Tray |
| Pengaturan | layar settings | jendela pengaturan, atau file JSON dan perintah `flags` |
| Discord Rich Presence | ada | ada (tanpa tombol kustom dari BloxstrapRPC) |
| Pilih GPU (dropdown) | tidak ada | ada |
| Setup awal (Welcome, Install, Completion) | ada | ada |
| Halaman Appearance | tema, bootstrapper, ikon | tema (sistem/terang/gelap) dan judul bootstrapper |
| Export diagnostic data | ada | ada |
| Multi Account (beberapa akun, beberapa jendela Roblox) | tidak ada | ada: halaman Multi Account |
| Susun jendela Roblox jadi kotak rapi di layar | tidak ada | ada: Multi Account > Tata letak jendela, atau menu tray |
| Tetap di System Tray setelah Roblox jalan | tidak ada | ada (bisa dimatikan di Behaviour) |
| Overlay CPU / GPU / RAM di atas game (latar transparan) | tidak ada | ada: Pengaturan > Integrasi |
| Update diri sendiri | ada | ada: banner di dalam app, tanpa unduh lewat browser |
| Uninstall | Apps & Features dan tombol di app | sama: Apps & Features, tombol di app, atau `ruststrap uninstall` |

## Multi Account dan System Tray

Halaman **Multi Account** menyimpan beberapa akun Roblox dan menjalankan Roblox sebagai salah satunya.
Tombol **Tambah akun** punya tiga cara masuk:

1. **Masuk lewat Roblox**: jendela Roblox terbuka (passkey dan verifikasi 2 langkah bisa dipakai di situ).
2. **Password**: username dan password. Kalau Roblox minta captcha atau kode, pakai cara 1.
3. **Tempel cookie**: tempel cookie `.ROBLOSECURITY`.

Cookie dienkripsi dengan Windows DPAPI (hanya bisa dibuka oleh user Windows yang sama di PC yang sama) dan
disimpan di `Accounts.json`. Isinya tidak dikirim ke mana pun selain roblox.com.

Supaya beberapa akun tidak saling menutup, Ruststrap memegang dua nama kunci Roblox
(`ROBLOX_singletonMutex` dan `ROBLOX_singletonEvent`) di satu thread yang tidak pernah berhenti, sebelum Roblox
pertama dijalankan. Kalau ada Roblox yang sudah terbuka lebih dulu (tidak lewat Ruststrap), ia memegang kunci itu:
tutup semua jendela Roblox, lalu jalankan akun dari Ruststrap. Log ada di `Singleton::Roblox`.

Setelah Roblox jalan, Ruststrap tidak tertutup penuh: proses kecil `ruststrap -tray` tetap hidup sebagai ikon
di System Tray (menu: buka Ruststrap, Multi Account, tutup semua jendela Roblox, keluar). Proses itu juga
memegang mutex Roblox, jadi beberapa akun bisa main bersamaan. Matikan lewat **Behaviour** (tray) atau
**Multi Account** (beberapa jendela). Untuk beberapa jendela, jalankan akun pertama lewat Ruststrap
(bukan lewat Roblox yang sudah terbuka lebih dulu).

## Group akun

Di halaman **Multi Account**, bagian **Group**: kumpulkan beberapa akun dalam satu group yang punya nama
(misalnya "Tim Farm"). Tombol **Group baru** minta nama dan akun mana saja yang masuk. Tombol **Jalankan semua**
membuka Roblox untuk setiap akun di group, jadi tidak perlu menjalankan satu per satu.

- Akun dibuka bergantian, selang 6 detik (`GROUP_GAP_SECS` di `src/accounts.rs`), supaya tiap jendela sempat terbuka.
- Tiap group bisa punya game sendiri (opsional): Place ID atau link game, teman yang diikuti, dan Server ID. Kalau ketiganya kosong, group memakai pengaturan **Game** di halaman Multi Account. Ikut pemain lebih diutamakan daripada Place ID.
- Kalau satu akun gagal (cookie kedaluwarsa), akun lain tetap jalan. Penyebabnya ditulis di log (`Accounts::Group`).
- Group disimpan di `AccountGroups.json`. Hapus akun otomatis mengeluarkannya dari semua group.

## Susun jendela Roblox

Di halaman **Multi Account**, bagian **Tata letak jendela**:

- **Susun sekarang** menyusun semua jendela Roblox yang terbuka menjadi kotak di layar (area kerja, tanpa taskbar).
  Jendela pertama yang dibuka ada di kiri atas.
- **Kolom** (otomatis atau 1 sampai 6), **Jarak antar jendela** (0, 4, 8, 16 px), dan **Jendela tanpa bingkai**.
- **Susun otomatis**: tersusun lagi tiap ada jendela Roblox yang dibuka atau ditutup. Jalan di proses tray,
  jadi Ruststrap harus tetap di System Tray.
- Menu tray juga punya **Susun jendela Roblox**.

## Anti-AFK

Di halaman **Multi Account**: tiap beberapa menit (5 sampai 18), setiap jendela Roblox yang tidak di depan maju sebentar,
menerima satu input kecil, lalu jendelamu dikembalikan. Jendela yang diminimalkan diminimalkan lagi. Pilihan aksi:
**Geser mouse** (disarankan, tidak menyentuh karakter, jadi auto-farm tetap jalan), **Lompat** (Space), atau
**Zoom kamera** (I lalu O). Kalau kamu sedang mengetik atau menggerakkan mouse, ia menunggu jeda (paling lama 3 menit).
Jalan di proses tray.

## Max alts

Di halaman **Multi Account**: jendela Roblox yang tidak kamu mainkan dibuat ringan. Jendela paling depan tetap penuh
(dengan prioritas dari **Process priority**). Jendela Roblox lain dapat prioritas CPU Idle dan mode hemat
(efficiency mode) Windows. Pilihan **Pangkas RAM jendela latar** juga mengambil memori yang tidak terpakai tiap 20 detik.
Kalau tidak ada jendela Roblox di depan (kamu di browser), semua jendela dianggap latar.
Jalan di proses tray, dan semuanya kembali normal saat dimatikan atau saat tray ditutup.
Hanya kontrol proses biasa Windows yang dipakai (seperti di Task Manager), tanpa admin.

## Ikut pemain

Di halaman **Multi Account**, kolom **Ikut pemain**: isi username Roblox (atau link `roblox.com/users/...`).
Tombol **Luncurkan** lalu memasukkan akun ke server tempat pemain itu berada, dan Place ID serta ID server diabaikan.
Pemain harus online dan mengizinkan kamu bergabung (biasanya harus berteman). Pakai di beberapa akun untuk membawa
semuanya ke server yang sama. Kosongkan kolom untuk kembali ke cara biasa.

Roblox harus dalam mode jendela. Mode layar penuh (F11) bisa melawan pergeseran jendela.

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
ruststrap update                  cek GitHub dan pasang versi baru
ruststrap uninstall               hapus semuanya
```

Setelah `install`, klik "Play" di situs Roblox akan membuka game lewat Ruststrap.

### Update

Saat jendela Ruststrap dibuka (menu utama atau pengaturan), Ruststrap cek rilis terbaru di
<https://github.com/nightbest001/Ruststarp/releases>. Kalau ada versi lebih baru, muncul banner
**Update available: version X** dengan tombol **Update to X**. Klik tombol itu: file diunduh dengan bar progres,
diperiksa (ukuran, header program, dan checksum SHA-256 kalau GitHub menyediakannya), lalu menggantikan
`Ruststrap.exe` yang terpasang dan app dibuka ulang. Tidak perlu unduh lewat browser.

- Matikan cek otomatis: halaman **Ruststrap** di pengaturan, opsi *Check for Ruststrap updates*. Tombol **Check now** di halaman yang sama tetap bisa dipakai.
- Cek dan pasang dari terminal: `ruststrap update`.
- Hanya salinan yang terpasang yang bisa update. File `.exe` yang baru diunduh dan belum dipasang menampilkan setup wizard.
- Untuk merilis: buat tag `vX.Y.Z`. Workflow membangun dan melampirkan `ruststrap.exe` ke rilis, dan nama file itu yang dicari updater.
- File lama diganti nama jadi `Ruststrap.exe.old` lalu dihapus di start berikutnya.

### Uninstall

Tiga cara, hasilnya sama (hapus Ruststrap, shortcut, entri registry, dan Roblox yang dipasangnya, lalu kembalikan link
`roblox://` ke Roblox resmi kalau masih ada):

1. **Pengaturan Windows > Apps > Ruststrap > Uninstall**. Muncul dialog konfirmasi, tanpa terminal.
2. Di Ruststrap: halaman **Ruststrap** > **Uninstall**. Ada pilihan *Keep my settings and mods*.
3. Terminal: `ruststrap uninstall` (tambah `-quiet` supaya tanpa tanya, `-keepdata` untuk menyimpan pengaturan dan mod).

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
src/updater.rs           cek rilis GitHub, unduh, ganti exe sendiri
src/registry.rs          protokol roblox:// dan entri uninstall
src/watcher.rs           proses latar yang menunggu Roblox tutup
src/activity_watcher.rs  baca log Roblox: masuk dan keluar game
src/models.rs            Settings, State, manifest paket
```

## Yang belum ada

Gaya bootstrapper dan ikon kustom, bahasa, analytics, extract ikon, preset mod
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

## Bahasa

Jendela mendukung **English** dan **Bahasa Indonesia**. Ganti di Pengaturan > Tampilan > Bahasa
(default: ikut bahasa Windows). Teks UI ada di `ui-web/strings.json` (EN) dan `ui-web/strings.id.json` (ID);
setelah mengedit, jalankan `python3 ui-web/build.py`. Pesan dari sisi Rust ada di `src/lang.rs` (`pick("English", "Indonesia")`).

## Font kustom

Di Settings > Mods > Lain-lain: **Pilih font...** (file `.ttf` atau `.otf`) mengganti semua font di Roblox, **Hapus font yang dipakai** mengembalikannya.

- Font disalin ke `Modifications\content\fonts\CustomFont.ttf`, lalu setiap file `content\fonts\families\*.json` ditulis ulang supaya semua face menunjuk ke font itu.
- Sebelum ada yang diubah, file font asli Roblox dicadangkan ke `FontBackup\<versi>\`. Saat font dihapus, file asli dikembalikan dari cadangan.
- File dicek dulu (tabel `head`, `cmap`, `name`, outline). Kalau bukan font yang valid, ditolak.
- Berlaku saat Roblox dijalankan lewat Ruststrap berikutnya.

## Overlay performa

Di Pengaturan > Integrasi ada empat toggle: **Tampilkan FPS**, **Tampilkan CPU**, **Tampilkan GPU**, **Tampilkan RAM**. Kalau salah satu aktif,
proses kecil `ruststrap -overlay` jalan bareng Roblox dan menampilkan teks di pojok kiri atas layar. Latarnya transparan
penuh, klik tembus ke game, dan jendelanya menutup sendiri setelah Roblox tutup. Tombol **Pratinjau** menampilkannya 8 detik.

- CPU dari `GetSystemTimes`, RAM dari `GlobalMemoryStatusEx`, GPU dari counter `\GPU Engine(*)\Utilization Percentage` (sama seperti Task Manager, Windows 10 1709 ke atas).
- Cuma tampil di mode jendela dan borderless, tidak di atas layar penuh eksklusif.
- FPS dihitung seperti PresentMon: sesi ETW mendengar event `Present` provider DXGI milik proses Roblox. Sesi ETW butuh administrator, jadi `ruststrap -fpsmeter` jalan lewat UAC dan menulis angkanya ke `fps.txt` tiap detik. Overlay (user biasa) cuma membaca file itu. Hanya Direct3D, Vulkan tidak terhitung. Kalau UAC ditolak, FPS tampil "N/A".
