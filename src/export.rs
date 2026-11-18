use crate::{
    fonts,
    pdf::{save_atomic, Document, Layout, Run},
    session::rows,
};
use anyhow::{bail, ensure, Context, Result};
use eframe::egui::Rect;
use std::{
    collections::HashMap,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
};
use zip::{write::SimpleFileOptions, ZipWriter};

fn xml(s: &str) -> String {
    s.chars()
        .filter(|c| *c >= ' ' || matches!(c, '\n' | '\r' | '\t'))
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
struct Package(ZipWriter<Cursor<Vec<u8>>>);
impl Package {
    fn new() -> Self {
        Self(ZipWriter::new(Cursor::new(vec![])))
    }
    fn add(&mut self, name: &str, data: impl AsRef<[u8]>) -> Result<()> {
        self.0.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        self.0.write_all(data.as_ref())?;
        Ok(())
    }
    fn save(self, path: &Path) -> Result<()> {
        save_atomic(path, &self.0.finish()?.into_inner())
    }
}
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const OFFICE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const TYPES: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
fn relationship(id: &str, kind: &str, target: &str) -> String {
    format!(r#"<Relationship Id="{id}" Type="{OFFICE}/{kind}" Target="{target}"/>"#)
}
fn rels(entries: &str) -> String {
    format!(r#"<Relationships xmlns="{REL}">{entries}</Relationships>"#)
}
fn types(extra: &str) -> String {
    format!(
        r#"<Types xmlns="{TYPES}"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>{extra}</Types>"#
    )
}
fn run_xml(r: &Run, scale: f32, stretch: i32, family: &str) -> String {
    let rgb = r.rgb;
    let sz = (r.size * r.matrix.d.abs().max(0.001) * scale * 2.)
        .round()
        .max(1.) as i32;
    let name = xml(family);
    let bold = if r.bold { "<w:b/>" } else { "" };
    let italic = if r.italic { "<w:i/>" } else { "" };
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="{name}" w:hAnsi="{name}" w:eastAsia="{name}" w:cs="{name}"/>{bold}{italic}<w:color w:val="{:02X}{:02X}{:02X}"/><w:w w:val="{stretch}"/><w:sz w:val="{sz}"/><w:szCs w:val="{sz}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r>"#,
        rgb[0],
        rgb[1],
        rgb[2],
        xml(&r.text)
    )
}
fn section(w: f32, h: f32) -> String {
    format!(
        r#"<w:sectPr><w:type w:val="nextPage"/><w:pgSz w:w="{}" w:h="{}"/><w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>"#,
        (w * 20.).round() as u32,
        (h * 20.).round() as u32
    )
}
fn text_box(id: &str, r: &Run, scale: f32, font: &fonts::Face, bytes: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(bytes, font.index).ok()?;
    let units = face.units_per_em() as f32;
    let size = r.size * r.matrix.d * scale;
    let leading = ((face.ascender() - face.descender()) as f32 / units * size).max(size);
    let natural: f32 = r
        .text
        .chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .map(|u| u as f32 / units * size)
        })
        .collect::<Option<Vec<_>>>()?
        .iter()
        .sum();
    let left = r.origin.x * scale;
    let top = r.origin.y * scale - leading * 0.8;
    let width = (r.rect.right() - r.origin.x) * scale;
    let run = run_xml(
        r,
        scale,
        (width / natural.max(0.01) * 100.).round().clamp(1., 600.) as i32,
        &font.family,
    );
    Some(format!(
        r#"<w:r><w:pict><v:rect id="{id}" filled="f" stroked="f" style="position:absolute;margin-left:{left:.3}pt;margin-top:{top:.3}pt;width:{:.3}pt;height:{:.3}pt;z-index:1;mso-position-horizontal-relative:page;mso-position-vertical-relative:page;v-text-anchor:top"><v:textbox inset="0,0,0,0"><w:txbxContent><w:p><w:pPr><w:snapToGrid w:val="0"/><w:spacing w:before="0" w:after="0" w:line="{}" w:lineRule="exact"/></w:pPr>{run}</w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r>"#,
        width + 4.,
        size * 2.2,
        (leading * 20.).round() as u32
    ))
}
fn background(id: usize, w: f32, h: f32) -> String {
    let cx = (w * 12700.).round() as u64;
    let cy = (h * 12700.).round() as u64;
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="0" behindDoc="1" locked="0" layoutInCell="0" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="{cx}" cy="{cy}"/><wp:wrapNone/><wp:docPr id="{id}" name="Page {id}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="page.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="img{id}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#
    )
}
fn png(image: image::RgbaImage) -> Result<Vec<u8>> {
    let mut b = Cursor::new(vec![]);
    image::DynamicImage::ImageRgba8(image).write_to(&mut b, image::ImageFormat::Png)?;
    Ok(b.into_inner())
}
fn word_table(table: &crate::tables::Table) -> String {
    let mut body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:type="dxa" w:w="{}"/><w:tblInd w:type="dxa" w:w="{}"/><w:tblBorders>"#,
        (table.rect.width() * 20.) as u32,
        (table.rect.left() * 20.) as i32
    );
    for side in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        body += &format!(r#"<w:{side} w:val="single" w:sz="4" w:color="000000"/>"#);
    }
    body += "</w:tblBorders><w:tblLayout w:type=\"fixed\"/></w:tblPr><w:tblGrid>";
    for x in table.xs.windows(2) {
        body += &format!(r#"<w:gridCol w:w="{}"/>"#, ((x[1] - x[0]) * 20.) as u32);
    }
    body += "</w:tblGrid>";
    for (y, row) in table.cells.iter().enumerate() {
        body += &format!(
            r#"<w:tr><w:trPr><w:trHeight w:val="{}" w:hRule="atLeast"/></w:trPr>"#,
            ((table.ys[y + 1] - table.ys[y]) * 20.) as u32
        );
        for (x, value) in row.iter().enumerate() {
            body += &format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="{}" w:type="dxa"/></w:tcPr><w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
                ((table.xs[x + 1] - table.xs[x]) * 20.) as u32,
                xml(value)
            );
        }
        body += "</w:tr>";
    }
    body += "</w:tbl>";
    body
}
pub fn word(doc: &Document, pages: &[usize], path: &Path) -> Result<()> {
    ensure!(doc.extractable(), "이 PDF는 추출 권한이 제한되어 있습니다.");
    let mut package = Package::new();
    let mut body = String::new();
    let mut images = String::new();
    let mut fontdata: HashMap<PathBuf, Vec<u8>> = HashMap::new();
    for (seq, &i) in pages.iter().enumerate() {
        let page = doc.page(i)?;
        let layout = page.layout()?;
        let id = seq + 1;
        let scale = (1584. / page.size.max_elem()).min(1.);
        let w = page.size.x * scale;
        let h = page.size.y * scale;
        let mut placed = String::new();
        let mut hidden = vec![];
        for run in &layout.runs {
            if !run.safe_overlay
                || run.text.trim().is_empty()
                || layout
                    .graphics
                    .iter()
                    .any(|(n, r, _)| *n > run.index && r.intersects(run.rect))
                || layout.runs.iter().any(|other| {
                    other.index > run.index
                        && other.rect.shrink(0.6).intersects(run.rect.shrink(0.6))
                })
            {
                continue;
            }
            if let Some(font) = fonts::find(&run.family, run.bold, run.italic) {
                let bytes = fontdata
                    .entry(font.path.clone())
                    .or_insert_with(|| std::fs::read(&font.path).unwrap_or_default());
                if let Some(shape) =
                    text_box(&format!("Text{id}_{}", run.index), run, scale, font, bytes)
                {
                    placed.push_str(&shape);
                    hidden.push(run.index)
                }
            }
        }
        let sect = section(w, h);
        let tables = crate::tables::ruled(&layout);
        let grid_only = !tables.is_empty()
            && layout.graphics.iter().all(|(_, r, t)| {
                *t == 2
                    && tables
                        .iter()
                        .any(|tab| tab.rect.expand(1.).contains_rect(*r))
            });
        let flowing = (layout.graphics.is_empty() || grid_only)
            && !layout.runs.is_empty()
            && hidden.len() == layout.runs.len()
            && scale == 1.;
        if flowing {
            let mut previous = 0.;
            let mut emitted = std::collections::HashSet::new();
            for line in rows(&layout.runs) {
                if let Some((idx, table)) = tables
                    .iter()
                    .enumerate()
                    .find(|(_, t)| t.rect.contains(line[0].rect.center()))
                {
                    if emitted.insert(idx) {
                        let gap = (table.rect.top() - previous).max(1.);
                        body += &format!(
                            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="{}" w:lineRule="exact"/></w:pPr></w:p>"#,
                            (gap * 20.) as u32
                        );
                        body += &word_table(table);
                        previous = table.rect.bottom();
                    }
                    continue;
                }
                let bbox = line.iter().fold(Rect::NOTHING, |r, t| r.union(t.rect));
                let before = (bbox.top() - previous).max(0.);
                let height = bbox.height().max(line[0].size);
                body.push_str(&format!(r#"<w:p><w:pPr><w:spacing w:before="{}" w:after="0" w:line="{}" w:lineRule="exact"/><w:ind w:left="{}"/></w:pPr>"#,(before*20.) as u32,(height*20.) as u32,(bbox.left().max(0.)*20.) as u32));
                for r in line {
                    body.push_str(&run_xml(r, 1., 100, &r.family));
                }
                body.push_str("</w:p>");
                previous = bbox.top() + height;
            }
        } else {
            // Deactivate only eligible foreground text on this temporary page. PDFium retains clipping, masks and paint order.
            let drawing = (|| -> Result<String> {
                for &index in &hidden {
                    page.set_active(index, false)?;
                }
                let pix = png(page.render(300. / 72.)?)?;
                package.add(&format!("word/media/page{id}.png"), pix)?;
                Ok(background(id, w, h))
            })();
            for &index in &hidden {
                page.set_active(index, true)?;
            }
            images.push_str(&relationship(
                &format!("img{id}"),
                "image",
                &format!("media/page{id}.png"),
            ));
            body.push_str(&format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="20" w:lineRule="exact"/></w:pPr>{}{placed}</w:p>"#,drawing?));
        }
        if id == pages.len() {
            body.push_str(&sect)
        } else {
            body.push_str(&format!(
                "<w:p><w:pPr><w:spacing w:line=\"20\" w:lineRule=\"exact\"/>{sect}</w:pPr></w:p>"
            ))
        }
    }
    let document = format!(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="{OFFICE}" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}</w:body></w:document>"#
    );
    package.add("[Content_Types].xml",types(r#"<Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>"#))?;
    package.add(
        "_rels/.rels",
        rels(&relationship("doc", "officeDocument", "word/document.xml")),
    )?;
    package.add("word/_rels/document.xml.rels", rels(&images))?;
    package.add("word/document.xml", document)?;
    package.save(path)
}
fn cells(layout: &Layout) -> Vec<Vec<String>> {
    let mut table = vec![];
    for row in rows(&layout.runs) {
        let mut cells: Vec<String> = vec![];
        let mut end = f32::NEG_INFINITY;
        for run in row {
            let gap = run.rect.left() - end;
            if gap > run.size * 0.9 || cells.is_empty() {
                cells.push(run.text.clone())
            } else {
                let v = cells.last_mut().unwrap();
                if gap > run.size * 0.15 {
                    v.push(' ')
                }
                v.push_str(&run.text)
            }
            end = run.rect.right();
        }
        if cells.len() > 1 {
            table.push(cells)
        }
    }
    table
}
fn col(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}
pub fn excel(doc: &Document, pages: &[usize], path: &Path) -> Result<()> {
    ensure!(doc.extractable(), "이 PDF는 추출 권한이 제한되어 있습니다.");
    let mut package = Package::new();
    let mut sheetlist = String::new();
    let mut links = String::new();
    let mut ct = String::from(
        r#"<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
    );
    const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let mut count = 0;
    for &p in pages {
        let layout = doc.page(p)?.layout()?;
        let ruled = crate::tables::ruled(&layout);
        let grids = if ruled.is_empty() {
            vec![cells(&layout)]
        } else {
            ruled.into_iter().map(|t| t.cells).collect()
        };
        for grid in grids {
            if grid.is_empty() {
                continue;
            }
            count += 1;
            let mut data = String::new();
            for (r, row) in grid.iter().enumerate() {
                let n = r + 1;
                data.push_str(&format!(r#"<row r="{n}">"#));
                for (c, value) in row.iter().enumerate() {
                    data.push_str(&format!(
                        r#"<c r="{}{n}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
                        col(c),
                        xml(value)
                    ))
                }
                data.push_str("</row>")
            }
            package.add(&format!("xl/worksheets/sheet{count}.xml"),format!(r#"<worksheet xmlns="{S}"><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" state="frozen"/></sheetView></sheetViews><sheetFormatPr defaultColWidth="18" defaultRowHeight="15"/><sheetData>{data}</sheetData></worksheet>"#))?;
            sheetlist += &format!(
                r#"<sheet name="p{}_table{count}" sheetId="{count}" r:id="s{count}"/>"#,
                p + 1
            );
            links += &relationship(
                &format!("s{count}"),
                "worksheet",
                &format!("worksheets/sheet{count}.xml"),
            );
            ct += &format!(
                r#"<Override PartName="/xl/worksheets/sheet{count}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
            );
        }
    }
    ensure!(count > 0, "선택한 페이지에서 표를 찾지 못했습니다.");
    package.add("[Content_Types].xml", types(&ct))?;
    package.add(
        "_rels/.rels",
        rels(&relationship("book", "officeDocument", "xl/workbook.xml")),
    )?;
    package.add("xl/_rels/workbook.xml.rels", rels(&links))?;
    package.add(
        "xl/workbook.xml",
        format!(
            r#"<workbook xmlns="{S}" xmlns:r="{OFFICE}"><sheets>{sheetlist}</sheets></workbook>"#
        ),
    )?;
    package.save(path)
}
pub fn image_pages(doc: &Document, pages: &[usize], dir: &Path, jpeg: bool) -> Result<()> {
    ensure!(doc.extractable(), "이 PDF는 추출 권한이 제한되어 있습니다.");
    let tmp = tempfile::tempdir_in(dir.parent().context("출력 폴더")?)?;
    for &p in pages {
        let img = image::DynamicImage::ImageRgba8(doc.page(p)?.render(160. / 72.)?);
        let ext = if jpeg { "jpg" } else { "png" };
        let path = tmp.path().join(format!("page_{:04}.{ext}", p + 1));
        if jpeg {
            img.to_rgb8().save(path)?
        } else {
            img.save(path)?
        }
    }
    ensure!(!dir.exists(), "대상 폴더가 이미 있습니다.");
    std::fs::rename(tmp.path(), dir)?;
    Ok(())
}
pub fn office(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let script = include_str!("../office_convert.ps1");
    let temp = tempfile::tempdir()?;
    let output = temp.path().join("converted.pdf");
    let mut child = Command::new(
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("Windows")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe"),
    );
    let result = child
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("SIMPLEPDF_OFFICE_INPUT", std::path::absolute(source)?)
        .env("SIMPLEPDF_OFFICE_OUTPUT", &output)
        .creation_flags(0x08000000)
        .output()?;
    if !result.status.success() {
        bail!(
            "Office 변환 실패: {}",
            String::from_utf8_lossy(&result.stderr)
        )
    }
    let bytes = std::fs::read(output)?;
    Document::open(bytes.clone(), "")?;
    save_atomic(target, &bytes)
}
