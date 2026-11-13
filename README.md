<p align="center"><img src="assets/Ruststrap-full-dark.png" width="360" alt="Ruststrap"></p>

# Ruststrap

Repo: https://github.com/nightbest001/Ruststarp

Ruststrap is a Rust port of [Bloxstrap](https://github.com/bloxstraplabs/bloxstrap), an alternative bootstrapper
for the Roblox game client. It downloads and updates Roblox, lets you modify its files, and sets FastFlags.

> **Status:** early port. It has not been compiled or tested yet (the machine that wrote it had no Rust
> toolchain). Run `cargo build --release` on Windows and expect to fix a few compile errors.

## What is ported

| Bloxstrap | Ruststrap |
|---|---|
| `Bootstrapper.cs` (install, update, package download, MD5 check, extract, launch) | `src/bootstrapper.rs` |
| `RobloxInterfaces/Deployment.cs` (mirror race, channels) | `src/deployment.rs` |
| `LaunchSettings.cs` (`-player`, `-studio`, `roblox:` URIs) | `src/launch_settings.rs` |
| `Settings/State/DistributionState`, `JsonManager.cs` | `src/models.rs`, `src/json_manager.rs` |
| `FastFlagManager.cs` | `src/fast_flags.rs` (`ruststrap flags ...`) |
| `Modifications` folder handling | `Bootstrapper::apply_modifications` |
| `Installer.cs`, `WindowsRegistry.cs` | `src/installer.rs`, `src/registry.rs` |
| `Watcher.cs`, `ActivityWatcher.cs` | `src/watcher.rs`, `src/activity_watcher.rs` |
| `Paths.cs`, `Logger.cs` | `src/paths.rs`, `src/logger.rs` |

## Not ported (yet)

- The WPF/WinForms interface (bootstrapper dialogs, settings window, tray icon). Ruststrap is CLI + log based.
- Discord Rich Presence, custom bootstrapper themes, in-app self-update, emoji/cursor/font mod presets,
  shortcuts, locale files, background updater, WebView2 auto-install.

## Usage

```
ruststrap install              copy to %LOCALAPPDATA%\Ruststrap, register roblox:// links
ruststrap -player              launch Roblox (updates first)
ruststrap -studio              launch Roblox Studio
ruststrap -player -nolaunch    update only
ruststrap flags set DFIntTaskSchedulerTargetFps 144
ruststrap flags preset msaa 4
ruststrap uninstall
```

Data lives in `%LOCALAPPDATA%\Ruststrap` (`Settings.json`, `Modifications\`, `Versions\`, `Logs\`).
Set `RUSTSTRAP_DEBUG=1` to also print log lines to the console.

## Build

```
cargo build --release
```

Windows only. License: MIT (same as Bloxstrap). Original work by pizzaboxer and the Bloxstrap contributors.
