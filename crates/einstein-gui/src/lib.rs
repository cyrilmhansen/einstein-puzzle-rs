//! Native Eframe shell for the reconstruction.
#![forbid(unsafe_code)]
mod app;
mod board;
mod navigation;
mod theme;

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;

const TITLE: &str = "Einstein Puzzle — Reconstruction";
const USAGE: &str = "Usage : einstein-gui [--size LARGEURxHAUTEUR] [--seed ENTIER] [--save-dir DOSSIER] [--classic]\n";

struct Options {
    size: [f32; 2],
    seed: u64,
    save_dir: PathBuf,
    classic: bool,
}

fn default_save_dir() -> Result<PathBuf, &'static str> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .ok_or("définir HOME ou fournir --save-dir")?;
    Ok(home.join("einstein-rust/parties"))
}

fn options(args: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, &'static str> {
    let (mut size, mut seed, mut dir) = (None, None, None);
    let mut classic = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--classic") if !classic => classic = true,
            Some("--size") if size.is_none() => {
                let raw = args.next().ok_or("--size attend LARGEURxHAUTEUR")?;
                let (w, h) = raw
                    .to_str()
                    .and_then(|s| s.split_once('x'))
                    .ok_or("taille invalide")?;
                let dimension = |s: &str, min: u16| {
                    s.parse::<u16>()
                        .ok()
                        .filter(|n| (min..=8192).contains(n))
                        .map(f32::from)
                        .ok_or("taille hors limites")
                };
                size = Some([dimension(w, 640)?, dimension(h, 480)?]);
            }
            Some("--seed") if seed.is_none() => {
                seed = Some(
                    args.next()
                        .and_then(|s| s.to_str()?.parse::<u64>().ok())
                        .ok_or("graine Rust 64 bits attendue")?,
                );
            }
            Some("--save-dir") if dir.is_none() => {
                dir = Some(PathBuf::from(
                    args.next()
                        .filter(|s| !s.is_empty())
                        .ok_or("dossier attendu")?,
                ));
            }
            _ => return Err("option inconnue ou répétée : consulter --help"),
        }
    }
    let seed = seed.unwrap_or_else(|| {
        let elapsed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        elapsed.as_secs().rotate_left(32)
            ^ u64::from(elapsed.subsec_nanos())
            ^ u64::from(std::process::id())
    });
    Ok(Some(Options {
        size: size.unwrap_or([1024.0, 768.0]),
        seed,
        save_dir: dir.map_or_else(default_save_dir, Ok)?,
        classic,
    }))
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let Some(options) = options(std::env::args_os().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(options.size)
            .with_min_inner_size([640.0, 480.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        TITLE,
        native,
        Box::new(|cc| {
            Ok(Box::new(app::App::new(
                cc,
                options.seed,
                options.save_dir,
                options.classic,
            )))
        }),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_line_controls_window_and_seed_without_overflow() {
        let ok = options(
            [
                "--size",
                "800x600",
                "--seed",
                "18446744073709551615",
                "--save-dir",
                "./test",
            ]
            .map(Into::into),
        )
        .unwrap()
        .unwrap();
        assert_eq!(ok.size, [800.0, 600.0]);
        assert_eq!(ok.seed, u64::MAX);
        assert_eq!(ok.save_dir, PathBuf::from("./test"));
        assert!(options(["--size", "800x200"].map(Into::into)).is_err());
        assert!(options(["--seed", "18446744073709551616"].map(Into::into)).is_err());
    }
}
