#![windows_subsystem = "windows"]
mod app;
mod export;
mod ffi;
mod fonts;
mod forms;
mod instance;
mod pdf;
mod pictures;
mod session;
mod tables;
mod toolbar;
mod verify;
use anyhow::{Context, Result};
use std::{io::Read, path::PathBuf};
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|s| s == "--self-test") {
        return verify::run(&PathBuf::from(args.get(1).context("검증 폴더")?));
    }
    if args.first().is_some_and(|s| s == "--convert") {
        anyhow::ensure!(args.len() >= 5, "변환 인수 누락");
        let kind = args[1].to_string_lossy();
        let input = PathBuf::from(&args[2]);
        let output = PathBuf::from(&args[3]);
        let result = (|| -> Result<()> {
            if kind == "office" {
                return export::office(&input, &output);
            }
            let mut pw = String::new();
            std::io::stdin().take(65536).read_to_string(&mut pw)?;
            let d = pdf::Document::open(std::fs::read(input)?, &pw)?;
            let pages = session::pages(&args[4].to_string_lossy(), d.count())?;
            match kind.as_ref() {
                "docx" => export::word(&d, &pages, &output),
                "xlsx" => export::excel(&d, &pages, &output),
                "pdf" => pdf::save_atomic(&output, &d.extract(&pages)?),
                "jpg" | "png" => export::image_pages(&d, &pages, &output, kind == "jpg"),
                _ => anyhow::bail!("지원하지 않는 형식"),
            }
        })();
        if let (Err(e), Some(path)) = (&result, args.get(5)) {
            let _ = std::fs::write(path, format!("{e:#}"));
        }
        return result;
    }
    let smoke = args.first().is_some_and(|s| s == "--ui-check");
    let (inbox, paths) = if smoke {
        let paths = args
            .get(1)
            .into_iter()
            .chain(args.iter().skip(3))
            .map(PathBuf::from)
            .collect();
        (None, paths)
    } else {
        let paths = args.iter().map(PathBuf::from).collect();
        let Some((inbox, paths)) = instance::Inbox::launch(paths)? else {
            return Ok(());
        };
        (Some(inbox), paths)
    };
    let shot = if smoke {
        Some(PathBuf::from(args.get(2).context("화면 검증 이미지")?))
    } else {
        None
    };
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../icon_pdf_preview.png"))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("SimplePDF")
            .with_inner_size([1150., 790.])
            .with_min_inner_size([700., 360.])
            .with_decorations(false)
            .with_icon(icon),
        ..Default::default()
    };
    eframe::run_native(
        "SimplePDF",
        options,
        Box::new(move |cc| {
            let mut app = app::App::new(cc, paths);
            if let Some(inbox) = inbox {
                inbox.watch(cc.egui_ctx.clone());
                app.inbox = Some(inbox);
            }
            Ok(match shot {
                Some(output) => {
                    Box::new(verify::WindowCheck::new(app, output)) as Box<dyn eframe::App>
                }
                None => Box::new(app),
            })
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
fn main() {
    if let Err(e) = run() {
        if std::env::args_os()
            .nth(1)
            .is_some_and(|a| a == "--convert" || a == "--self-test")
        {
            eprintln!("{e:#}");
        } else {
            rfd::MessageDialog::new()
                .set_title("SimplePDF")
                .set_description(format!("{e:#}"))
                .show();
        }
        std::process::exit(1)
    }
}
