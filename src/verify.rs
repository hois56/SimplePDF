use crate::{
    pdf::*,
    session::{self, BookmarkOp, Session},
};
use anyhow::{ensure, Result};
use eframe::egui::{pos2, vec2, Rect};
use std::path::Path;
pub fn run(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = tempfile::tempdir_in(dir)?;
    let mut passed = vec![];
    let result = (|| -> Result<()> {
        let mut s = Session::open(Document::blank()?.bytes()?, None, String::new())?;
        let rect = Rect::from_min_size(pos2(32., 40.), vec2(290., 47.));
        let style = Style {
            size: 18.,
            rgb: [25, 50, 80],
            font: None,
        };
        let edit = Edit {
            page: 0,
            runs: vec![],
            rect,
            text: "SimplePDF 한글 검색".into(),
            style: style.clone(),
        };
        s.apply(|d| d.page(0)?.edit(&edit))?;
        let layout = s.doc.page(0)?.layout()?;
        ensure!(
            layout.runs.len() == 1 && layout.runs[0].text == edit.text,
            "한글 입력 {:?}",
            layout.runs
        );
        let runs = session::edit_line(&layout, layout.runs[0].rect.center());
        let font_before = s.doc.page(0)?.font_bytes(runs[0].index);
        let edited = Edit {
            runs,
            text: "SimplePDF 한글 수정".into(),
            ..edit.clone()
        };
        s.apply(|d| d.page(0)?.edit(&edited))?;
        let layout = s.doc.page(0)?.layout()?;
        ensure!(
            layout.runs[0].text == edited.text,
            "수정한 문구: {:?}",
            layout.runs[0].text
        );
        ensure!(
            font_before == s.doc.page(0)?.font_bytes(layout.runs[0].index),
            "글꼴 유지"
        );
        passed.push("한글 입력·직접 수정·원본 글꼴");
        s.undo(false)?;
        ensure!(s.doc.page(0)?.layout()?.runs[0].text == edit.text);
        s.undo(true)?;
        let file = tmp.path().join("한글 편집.pdf");
        s.save(&file)?;
        ensure!(!s.dirty());
        let check = Document::open(std::fs::read(&file)?, "")?;
        ensure!(session::hits(&check.page(0)?.layout()?.glyphs, "한글 수정").len() == 1);
        passed.push("실행 취소·다시 실행·저장·검색");
        s.apply(|d| d.import(&check, &[0, 0]))?;
        ensure!(s.doc.count() == 3);
        s.apply(|d| d.rotate(1))?;
        s.apply(|d| d.move_page(1, 0))?;
        ensure!(s.doc.page(0)?.size.x > s.doc.page(0)?.size.y, "페이지 이동");
        let part = Document::open(s.doc.extract(&[1, 0, 1])?, "")?;
        ensure!(part.count() == 3 && part.page(1)?.size.x > part.page(1)?.size.y);
        s.apply(|d| d.delete(2))?;
        ensure!(s.doc.count() == 2);
        passed.push("병합·회전·이동·반복 범위 추출·삭제");
        s.bookmark(BookmarkOp::Add("첫 장".into(), 0, None))?;
        s.bookmark(BookmarkOp::Add("두 번째".into(), 1, Some(0)))?;
        s.bookmark(BookmarkOp::Rename(1, "다음 장".into()))?;
        ensure!(
            s.bookmarks.len() == 2
                && s.bookmarks[1].depth == 1
                && s.bookmarks[1].dest.as_ref().unwrap().page == 1
                && s.bookmarks[1].title == "다음 장"
        );
        s.bookmark(BookmarkOp::Remove(0))?;
        ensure!(s.bookmarks.is_empty());
        passed.push("계층 책갈피 추가·이름 변경·이동 대상·삭제");
        let img = tmp.path().join("picture.png");
        image::RgbaImage::from_pixel(120, 100, image::Rgba([210, 20, 39, 255])).save(&img)?;
        let pic = Document::open(Document::images(&[img])?, "")?;
        let image = pic.page(0)?.render(1.)?;
        let color = image.get_pixel(40, 40).0;
        ensure!(color[0] > 195 && color[2] < 60, "이미지 색상 {color:?}");
        for jpeg in [false, true] {
            let target = tmp.path().join(if jpeg { "jpeg" } else { "png" });
            crate::export::image_pages(&pic, &[0], &target, jpeg)?;
            ensure!(std::fs::read_dir(target)?.count() == 1);
        }
        passed.push("이미지 삽입·PDF 생성·PNG 및 JPEG 내보내기");
        crate::app::verify_page_scroll()?;
        passed.push("연속 휠·프레임 내 합산·방향 반전·첫 프레임 즉시 전환·두 페이지·정밀 스크롤·축소 및 DPI별 픽셀 정렬");
        crate::app::verify_controls(&file)?;
        passed.push("화면 맞춤·휠 줌/스크롤·두 페이지·검색 단축키·책갈피 패널 경계");
        let second = tmp.path().join("여러 쪽.pdf");
        std::fs::write(&second, s.doc.bytes()?)?;
        crate::app::verify_tabs_and_outline(&file, &second)?;
        passed.push("책갈피 구간·접힌 상위 항목·복수 PDF 탭·편집 보존·한 줄 헤더");
        if dir.join("navigation.pdf").is_file() {
            let n = Document::open(std::fs::read(dir.join("navigation.pdf"))?, "")?;
            ensure!(n.bookmarks().len() >= 2);
            let p = n.page(2)?.layout()?;
            for (q, k) in [("needle", 4), ("STRASSE", 1), ("검색 대상", 1)] {
                ensure!(session::hits(&p.glyphs, q).len() == k, "검색 {q}");
            }
            let other = n.page(5)?.layout()?;
            for q in [
                "한국어검색",
                "international",
                "wrapped search query",
                "value[1].*",
            ] {
                ensure!(session::hits(&other.glyphs, q).len() == 1, "검색 {q}");
            }
            let page = n.page(1)?;
            let found = session::inferred(&page.layout()?, page.size.y, &n, &n.bookmarks());
            ensure!(!found.is_empty(), "인쇄 목차");
            passed.push("한글·유니코드·줄바꿈 검색과 인쇄 목차");
        }
        if dir.join("locked.pdf").is_file() {
            let bytes = std::fs::read(dir.join("locked.pdf"))?;
            ensure!(Document::open(bytes.clone(), "").is_err());
            let mut locked = Session::open(bytes, None, "reader-test".into())?;
            ensure!(!locked.doc.editable() && locked.apply(|d| d.rotate(0)).is_err());
            passed.push("암호 및 편집 권한");
        }
        if dir.join("table.pdf").is_file() {
            let d = Document::open(std::fs::read(dir.join("table.pdf"))?, "")?;
            let tables = crate::tables::ruled(&d.page(0)?.layout()?);
            ensure!(
                tables.len() == 1 && tables[0].cells[1][1] == "=literal",
                "표의 셀 인식"
            );
            crate::export::word(&d, &[0], &dir.join("rust-table.docx"))?;
            crate::export::excel(&d, &[0], &dir.join("rust-table.xlsx"))?;
            passed.push("표의 좌표 인식·Word 표·Excel 내보내기");
        }
        if dir.join("forms.pdf").is_file() {
            let bytes = std::fs::read(dir.join("forms.pdf"))?;
            let source = Document::open(bytes.clone(), "")?;
            let base = crate::forms::page_copy(&bytes, "", 0)?;
            let d = Document::open(base.clone(), "")?;
            ensure!(
                source.page(0)?.render(1.)? == d.page(0)?.render(1.)?,
                "양식 분리 전후 화면"
            );
            let runs = d.page(0)?.layout()?.runs;
            ensure!(runs.len() == 1);
            let style = Style {
                size: runs[0].size,
                rgb: runs[0].rgb,
                font: None,
            };
            let edit = Edit {
                page: 0,
                rect: runs[0].rect,
                runs,
                style,
                text: "Private form".into(),
            };
            let mut session = Session::open(bytes, None, String::new())?;
            session.edit(&edit, Some(&base))?;
            ensure!(session.doc.page(0)?.layout()?.runs[0].text == "Private form");
            ensure!(session.doc.page(1)?.render(1.)? == source.page(1)?.render(1.)?);
            passed.push("반복 배치한 양식 안의 문구를 한 곳만 수정");
        }
        Ok(())
    })();
    if let Err(err) = &result {
        passed.push("FAILED");
        std::fs::write(
            dir.join("rust-verification.txt"),
            format!("{}\n{err:#}", passed.join("\n")),
        )?;
    } else {
        std::fs::write(dir.join("rust-verification.txt"), passed.join("\n"))?;
    }
    result
}

pub struct WindowCheck {
    app: crate::app::App,
    output: std::path::PathBuf,
    frames: u32,
}
impl WindowCheck {
    pub fn new(app: crate::app::App, output: std::path::PathBuf) -> Self {
        Self {
            app,
            output,
            frames: 0,
        }
    }
}
impl eframe::App for WindowCheck {
    fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
        use eframe::egui::{Event, ViewportCommand};
        self.app.update(ctx, frame);
        self.frames += 1;
        if self.frames == 5 {
            ctx.send_viewport_cmd(ViewportCommand::Screenshot(Default::default()));
        }
        if let Some(image) = ctx.input(|i| {
            i.events.iter().find_map(|e| {
                if let Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        }) {
            let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            image::save_buffer(
                &self.output,
                &pixels,
                image.size[0] as u32,
                image.size[1] as u32,
                image::ColorType::Rgba8,
            )
            .expect("test screenshot");
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(70));
    }
}
