use crate::pdf::default_font;
use std::{path::PathBuf, sync::OnceLock};

#[derive(Clone)]
pub struct Face {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
    pub index: u32,
}
static FONTS: OnceLock<Vec<Face>> = OnceLock::new();
pub fn all() -> &'static [Face] {
    FONTS.get_or_init(|| {
        let mut out = vec![];
        let mut dirs = vec![default_font().parent().unwrap().to_path_buf()];
        if let Some(p) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(PathBuf::from(p).join("Microsoft/Windows/Fonts"))
        }
        for dir in dirs {
            let Ok(list) = std::fs::read_dir(dir) else {
                continue;
            };
            for item in list.flatten() {
                let path = item.path();
                let ext = path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
                    continue;
                }
                let Ok(data) = std::fs::read(&path) else {
                    continue;
                };
                for index in 0..ttf_parser::fonts_in_collection(&data).unwrap_or(1) {
                    let Ok(face) = ttf_parser::Face::parse(&data, index) else {
                        continue;
                    };
                    let name = |id| {
                        face.names()
                            .into_iter()
                            .filter(|n| n.name_id == id)
                            .find_map(|n| n.to_string())
                    };
                    if let Some(family) = name(16).or_else(|| name(1)) {
                        out.push(Face {
                            family,
                            style: name(17).or_else(|| name(2)).unwrap_or_default(),
                            path: path.clone(),
                            index,
                        })
                    }
                }
            }
        }
        out.sort_by(|a, b| a.family.cmp(&b.family).then(a.style.cmp(&b.style)));
        out
    })
}
pub fn find(name: &str, bold: bool, italic: bool) -> Option<&'static Face> {
    let key = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let mut needle = key(name.rsplit('+').next().unwrap_or(name));
    for s in [
        "bolditalicmt",
        "bolditalic",
        "regular",
        "boldmt",
        "italicmt",
        "psmt",
        "bold",
        "italic",
        "mt",
    ] {
        if let Some(n) = needle.strip_suffix(s) {
            needle = n.to_owned();
            break;
        }
    }
    match needle.as_str() {
        "helvetica" => needle = "arial".into(),
        "timesroman" => needle = "timesnewroman".into(),
        "courier" => needle = "couriernew".into(),
        _ => (),
    }
    all()
        .iter()
        .filter(|f| key(&f.family) == needle)
        .min_by_key(|f| {
            let st = f.style.to_lowercase();
            (bold != st.contains("bold")) as i32
                + (italic != (st.contains("italic") || st.contains("oblique"))) as i32
        })
}
