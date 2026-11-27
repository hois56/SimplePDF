use crate::{
    pdf::*,
    session::{self, BookmarkOp, Session},
};
use anyhow::{ensure, Context, Result};
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
        let codes = Document::blank()?;
        for (line, text) in ["e23 E23 23 E230 AE23 (E23)", "E", "23", "-23"]
            .iter()
            .enumerate()
        {
            codes.page(0)?.edit(&Edit {
                text: text.to_string(),
                rect: rect.translate(vec2(0., line as f32 * 45.)),
                ..edit.clone()
            })?;
        }
        let codes = Document::open(codes.bytes()?, "")?;
        let glyphs = codes.page(0)?.layout()?.glyphs;
        let line_end = glyphs
            .iter()
            .position(|g| matches!(g.ch, '\r' | '\n'))
            .context("선택 검증 줄바꿈")?;
        let next_e = glyphs[line_end..]
            .iter()
            .position(|g| g.ch == 'E')
            .context("다음 줄의 E")?
            + line_end;
        let bands = crate::app::selection_bands(&glyphs[1..=next_e]);
        let first_line = glyphs[1..line_end]
            .iter()
            .filter(|g| g.rect.is_positive())
            .fold(Rect::NOTHING, |r, g| r.union(g.rect));
        ensure!(
            bands == [first_line, glyphs[next_e].rect] && bands[0].bottom() < bands[1].top(),
            "공백 포함 두 줄의 부분 선택 띠"
        );
        passed.push("여러 줄 드래그 시 공백을 포함한 줄별 형광펜 표시");
        let defaults = session::SearchOptions::default();
        let results = session::hits(&glyphs, "e23", defaults);
        ensure!(
            results.len() == 5,
            "e23 코드 검색: {}건 (5건 예상)",
            results.len()
        );
        for boxes in results {
            let highlighted: String = glyphs
                .iter()
                .filter(|g| {
                    g.rect.is_positive() && boxes.iter().any(|b| b.contains(g.rect.center()))
                })
                .map(|g| g.ch)
                .collect();
            ensure!(
                highlighted.eq_ignore_ascii_case("e23"),
                "코드 강조 범위: {highlighted}"
            );
        }
        passed.push("e23 포함 검색·단독 숫자와 줄 경계 제외·강조 범위");
        for (query, match_case, expected) in [("e23", true, 1), ("E23", true, 4), ("23", false, 8)]
        {
            let options = session::SearchOptions { match_case };
            ensure!(
                session::hits(&glyphs, query, options).len() == expected,
                "대소문자·숫자 검색 {query}"
            );
        }
        let hex = Document::blank()?;
        hex.page(0)?.edit(&Edit {
            text: "0x282A 0X282a 282A 0x282A0 P0x282A A282A 0x282B".into(),
            ..edit.clone()
        })?;
        let hex = Document::open(hex.bytes()?, "")?;
        let hex_glyphs = hex.page(0)?.layout()?.glyphs;
        let result = session::hits(&hex_glyphs, "282a", defaults);
        ensure!(
            result.len() == 6,
            "hex substring should match in longer tokens"
        );
        let selected: Vec<String> = result
            .iter()
            .map(|boxes| {
                hex_glyphs
                    .iter()
                    .filter(|g| {
                        g.rect.is_positive() && boxes.iter().any(|b| b.contains(g.rect.center()))
                    })
                    .map(|g| g.ch)
                    .collect()
            })
            .collect();
        ensure!(
            selected == ["282A", "282a", "282A", "282A", "282A", "282A"],
            "highlighted tokens: {selected:?}"
        );
        ensure!(session::hits(&hex_glyphs, "0x282a", defaults).len() == 4);
        ensure!(
            session::hits(
                &hex_glyphs,
                "282a",
                session::SearchOptions { match_case: true }
            )
            .len()
                == 1
        );
        passed.push("접두어·접미어가 있는 문자열의 포함 검색 및 일치 부분 강조");
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
        let glyphs = check.page(0)?.layout()?.glyphs;
        ensure!(session::hits(&glyphs, "한글 수정", defaults).len() == 1);
        ensure!(
            session::hits(&glyphs, "한", defaults).len() == 1,
            "한글 부분 검색"
        );
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
        crate::app::verify_image_modes()?;
        passed.push("이미지 모드의 클릭·드래그, 원본 픽셀 크기 한 줄 표시·창 가장자리 배치, 보기·편집 우클릭 내보내기");
        verify_picture_replacement(tmp.path())?;
        passed.push(
            "회전·클리핑·겹침·공유 이미지 개별 교체·원본 해상도 PNG 내보내기·투명도·저장·실행 취소",
        );
        crate::app::verify_page_scroll()?;
        passed.push("연속 휠·프레임 내 합산·방향 반전·첫 프레임 즉시 전환·두 페이지·정밀 스크롤·축소 및 DPI별 픽셀 정렬");
        crate::app::verify_controls(&file)?;
        passed.push("기본·2배 확대에서 본문 정방향·역방향 드래그 선택과 복사, 페이지 위치 유지");
        passed.push("화면 맞춤·휠 줌/스크롤·두 페이지·연속 Enter/Shift+Enter 검색 순환·한영 글꼴 검색과 선택·문구별 실행 취소와 다시 실행·책갈피 패널 경계");
        let second = tmp.path().join("여러 쪽.pdf");
        std::fs::write(&second, s.doc.bytes()?)?;
        crate::app::verify_tabs_and_outline(&file, &second)?;
        passed.push("책갈피 구간·접힌 상위 항목·복수 PDF 탭·편집 보존·한 줄 헤더");
        if dir.join("navigation.pdf").is_file() {
            let n = Document::open(std::fs::read(dir.join("navigation.pdf"))?, "")?;
            ensure!(n.bookmarks().len() >= 2);
            let p = n.page(2)?.layout()?;
            for (q, k) in [("needle", 4), ("STRASSE", 1), ("검색 대상", 1)] {
                ensure!(session::hits(&p.glyphs, q, defaults).len() == k, "검색 {q}");
            }
            let other = n.page(5)?.layout()?;
            for q in [
                "한국어검색",
                "international",
                "wrapped search query",
                "value[1].*",
            ] {
                ensure!(
                    session::hits(&other.glyphs, q, defaults).len() == 1,
                    "검색 {q}"
                );
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

fn verify_picture_replacement(tmp: &Path) -> Result<()> {
    use lopdf::{dictionary, Dictionary, Stream};
    let mut fixture = lopdf::Document::load_mem(&Document::blank()?.bytes()?)?;
    let pid = fixture.get_pages()[&1];
    let photo = fixture.add_object(Stream::new(
        dictionary! {
            "Type"=>"XObject", "Subtype"=>"Image", "ColorSpace"=>"DeviceRGB",
            "Width"=>1,"Height"=>1,"BitsPerComponent"=>8
        },
        vec![200, 20, 30],
    ));
    let form = fixture.add_object(Stream::new(dictionary! {
        "Type"=>"XObject", "Subtype"=>"Form", "BBox"=>vec![0.into(),0.into(),80.into(),40.into()],
        "Resources"=>dictionary!{"XObject"=>dictionary!{"Photo"=>photo}}
    }, b"q 4 0 72 40 re W n 80 0 0 40 0 0 cm /Photo Do Q 0 g 37 18 6 4 re f".to_vec()));
    let content = fixture.add_object(Stream::new(
        Dictionary::new(),
        b"q 0 1 -1 0 120 100 cm /Tile Do Q q 1 0 0 1 170 100 cm /Tile Do Q".to_vec(),
    ));
    let dict = fixture.get_dictionary_mut(pid)?;
    dict.set(
        "Resources",
        dictionary! {"XObject"=>dictionary!{"Tile"=>form}},
    );
    dict.set("Contents", content);
    let mut data = vec![];
    fixture.save_to(&mut data)?;
    let mut session = Session::open(data, None, String::new())?;
    let page = session.doc.page(0)?;
    let targets = crate::pictures::placements(&fixture, 0, &page)?;
    ensure!(
        targets.len() == 2
            && targets[0].contains(page.display(100., 120.))
            && !targets[0].contains(page.display(100., 102.)),
        "회전과 클리핑 영역 선택"
    );
    let original = page.render(1.)?;
    drop(page);
    let exported = tmp.join("개별 이미지.png");
    ensure!(targets[0].pixels == [1, 1], "이미지 자체의 픽셀 크기");
    session.export_picture(0, &targets[0], &exported)?;
    let png = image::open(&exported)?.into_rgba8();
    ensure!(
        png.dimensions() == (1, 1) && png.get_pixel(0, 0).0 == [200, 20, 30, 255],
        "회전되고 잘린 사진의 원본 픽셀 추출"
    );
    let mut replacement = image::RgbaImage::from_fn(40, 40, |x, y| {
        image::Rgba(if !(10..30).contains(&y) {
            [0, 0, 255, 255]
        } else if x > 29 {
            [0, 210, 0, 128]
        } else {
            [0, 210, 0, 255]
        })
    });
    replacement.put_pixel(1, 11, image::Rgba([211, 43, 51, 255]));
    replacement.put_pixel(3, 27, image::Rgba([55, 127, 193, 255]));
    let expected = image::imageops::crop_imm(&replacement, 0, 10, 40, 20).to_image();
    session.replace_picture(0, &targets[0], &replacement.into())?;
    let saved = tmp.join("교체.pdf");
    session.save(&saved)?;
    let reopened = Document::open(std::fs::read(saved)?, "")?;
    let page = reopened.page(0)?;
    let result = page.render(1.)?;
    let source = crate::pictures::open(session.bytes(), &session.password)?;
    let updated = crate::pictures::placements(&source, 0, &page)?;
    ensure!(
        updated[0].pixels == [40, 20],
        "교체한 이미지의 비정사각 픽셀 크기"
    );
    session.export_picture(0, &updated[0], &exported)?;
    let extracted = image::open(&exported)?.into_rgba8();
    ensure!(
        extracted == expected,
        "원본 방향·해상도·투명도 픽셀 검증: {:?} / {:?}",
        extracted.get_pixel(39, 10),
        expected.get_pixel(39, 10)
    );
    ensure!(!session.dirty(), "이미지 내보내기 후 문서 상태");
    let pixel = |x, y| {
        let p = page.display(x, y);
        result.get_pixel(p.x as u32, p.y as u32).0
    };
    ensure!(
        pixel(100., 120.)[..3] == [0, 210, 0] && pixel(100., 140.)[..3] == [0, 0, 0],
        "교체된 이미지와 위쪽 개체 순서"
    );
    ensure!(
        pixel(82., 120.)[..3] == [0, 210, 0] && pixel(118., 120.)[..3] == [0, 210, 0],
        "중앙 자르기"
    );
    let translucent = pixel(100., 167.);
    ensure!(
        translucent[0] > 100 && translucent[0] < 155 && translucent[1] > 210,
        "교체 이미지 투명도"
    );
    ensure!(
        pixel(100., 102.)[..3] == [255, 255, 255] && pixel(190., 120.)[..3] == [200, 20, 30],
        "클리핑과 재사용된 사진 보존"
    );
    let area = targets[0].bounds().expand(1.);
    for (x, y, p) in result.enumerate_pixels() {
        if !area.contains(pos2(x as f32, y as f32)) {
            ensure!(original.get_pixel(x, y) == p, "교체 영역 밖 픽셀 보존");
        }
    }
    session.undo(false)?;
    ensure!(session.doc.page(0)?.render(1.)? == original);
    session.undo(true)?;
    ensure!(session.doc.page(0)?.render(1.)? == result);
    Ok(())
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
