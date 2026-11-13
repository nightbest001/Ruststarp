//! "Export diagnostic data": a zip with the settings and logs, for bug reports.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;

use crate::paths;

fn add_file(zw: &mut zip::ZipWriter<fs::File>, name: &str, path: &Path) -> io::Result<()> {
    let Ok(bytes) = fs::read(path) else { return Ok(()) };
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zw.start_file(name, opts)?;
    zw.write_all(&bytes)
}

pub fn export(dest: &Path, config: bool, logs: bool) -> io::Result<()> {
    let p = paths::get();
    let mut zw = zip::ZipWriter::new(fs::File::create(dest)?);
    if config {
        add_file(&mut zw, "Settings.json", &p.base.join("Settings.json"))?;
        add_file(&mut zw, "State.json", &p.base.join("State.json"))?;
        add_file(
            &mut zw,
            "ClientAppSettings.json",
            &p.modifications.join("ClientSettings").join("ClientAppSettings.json"),
        )?;
    }
    if logs {
        if let Ok(rd) = fs::read_dir(&p.logs) {
            for e in rd.flatten() {
                let path = e.path();
                if path.is_file() {
                    let name = format!("Logs/{}", e.file_name().to_string_lossy());
                    add_file(&mut zw, &name, &path)?;
                }
            }
        }
    }
    zw.finish()?;
    Ok(())
}
