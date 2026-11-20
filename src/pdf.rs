use crate::ffi::*;
use anyhow::{anyhow, bail, ensure, Context, Result};
use eframe::egui::{pos2, vec2, Pos2, Rect, Vec2};
use image::{DynamicImage, RgbaImage};
use std::{collections::HashSet, ffi::CString, path::Path, ptr::null_mut, sync::Once};

static INIT: Once = Once::new();
fn initialize() {
    INIT.call_once(|| unsafe { FPDF_InitLibrary() });
}
fn checked(handle: H, name: &str) -> Result<H> {
    ensure!(!handle.is_null(), "{name} 실패 (PDFium {})", unsafe {
        FPDF_GetLastError()
    });
    Ok(handle)
}
fn ok(result: i32, msg: &str) -> Result<()> {
    ensure!(result != 0, "{msg}");
    Ok(())
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn utf16_value(mut read: impl FnMut(*mut u16, u32) -> u32) -> String {
    let bytes = read(null_mut(), 0);
    if bytes < 2 || bytes > 16_777_216 {
        return String::new();
    }
    let mut buf = vec![0; (bytes as usize + 1) / 2];
    let n = read(buf.as_mut_ptr(), bytes) as usize / 2;
    let end = n.saturating_sub(1).min(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub struct Document {
    handle: H,
    _source: Vec<u8>,
}
impl Drop for Document {
    fn drop(&mut self) {
        unsafe { FPDF_CloseDocument(self.handle) }
    }
}
pub struct Page<'a> {
    h: H,
    doc: &'a Document,
    pub size: Vec2,
}
impl Drop for Page<'_> {
    fn drop(&mut self) {
        unsafe { FPDF_ClosePage(self.h) }
    }
}
struct Text(H);
impl Drop for Text {
    fn drop(&mut self) {
        unsafe { FPDFText_ClosePage(self.0) }
    }
}
struct Bitmap(H);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { FPDFBitmap_Destroy(self.0) }
    }
}
struct Font(H);
impl Drop for Font {
    fn drop(&mut self) {
        unsafe { FPDFFont_Close(self.0) }
    }
}
struct NewObject(H);
impl Drop for NewObject {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FPDFPageObj_Destroy(self.0) }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Destination {
    pub page: usize,
    pub point: Option<Pos2>,
}
#[derive(Clone, Debug)]
pub struct Bookmark {
    pub title: String,
    pub dest: Option<Destination>,
    pub depth: usize,
}
#[derive(Clone, Debug)]
pub struct Run {
    pub index: i32,
    pub text: String,
    pub rect: Rect,
    pub origin: Pos2,
    pub matrix: Matrix,
    pub size: f32,
    pub family: String,
    pub rgb: [u8; 3],
    pub bold: bool,
    pub italic: bool,
    pub safe_overlay: bool,
}
#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: char,
    pub rect: Rect,
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub runs: Vec<Run>,
    pub graphics: Vec<(i32, Rect, i32)>,
    pub rules: Vec<(Pos2, Pos2)>,
    pub glyphs: Vec<Glyph>,
}
#[derive(Clone)]
pub struct Style {
    pub size: f32,
    pub rgb: [u8; 3],
    pub font: Option<std::path::PathBuf>,
}
#[derive(Clone)]
pub struct Edit {
    pub page: usize,
    pub runs: Vec<Run>,
    pub rect: Rect,
    pub text: String,
    pub style: Style,
}

impl Document {
    fn empty() -> Result<Self> {
        initialize();
        Ok(Self {
            handle: checked(unsafe { FPDF_CreateNewDocument() }, "새 문서")?,
            _source: vec![],
        })
    }
    pub fn blank() -> Result<Self> {
        let doc = Self::empty()?;
        drop(doc.new_page(595., 842.)?);
        Ok(doc)
    }
    pub fn open(bytes: Vec<u8>, password: &str) -> Result<Self> {
        initialize();
        let pw = CString::new(password)?;
        let h = unsafe { FPDF_LoadMemDocument64(bytes.as_ptr(), bytes.len(), pw.as_ptr()) };
        if h.is_null() && unsafe { FPDF_GetLastError() } == 4 {
            bail!("PDF 암호를 확인하세요.")
        }
        let doc = Self {
            handle: checked(h, "PDF 열기")?,
            _source: bytes,
        };
        ensure!(doc.count() > 0, "빈 PDF입니다.");
        Ok(doc)
    }
    pub fn count(&self) -> usize {
        unsafe { FPDF_GetPageCount(self.handle).max(0) as usize }
    }
    pub fn editable(&self) -> bool {
        unsafe { FPDF_GetDocPermissions(self.handle) & 8 != 0 }
    }
    pub fn extractable(&self) -> bool {
        unsafe { FPDF_GetDocPermissions(self.handle) & 16 != 0 }
    }
    pub fn encrypted(&self) -> bool {
        unsafe { FPDF_GetSecurityHandlerRevision(self.handle) >= 0 }
    }
    fn wrap_page(&self, h: H) -> Result<Page<'_>> {
        let h = checked(h, "페이지 열기")?;
        let size = unsafe { vec2(FPDF_GetPageWidthF(h), FPDF_GetPageHeightF(h)) };
        let page = Page { doc: self, h, size };
        ensure!(
            size.x.is_finite() && size.y.is_finite() && size.x > 0. && size.y > 0.,
            "페이지 크기가 잘못되었습니다."
        );
        Ok(page)
    }
    pub fn page(&self, i: usize) -> Result<Page<'_>> {
        ensure!(i < self.count(), "페이지 번호를 확인하세요.");
        self.wrap_page(unsafe { FPDF_LoadPage(self.handle, i as i32) })
    }
    fn new_page(&self, w: f32, h: f32) -> Result<Page<'_>> {
        self.wrap_page(unsafe {
            FPDFPage_New(self.handle, self.count() as i32, w as f64, h as f64)
        })
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        unsafe extern "C" fn write(w: *mut Writer, p: *const u8, n: u32) -> i32 {
            if n == 0 {
                return 1;
            }
            if p.is_null() {
                return 0;
            }
            let v = &mut *((*w).sink as *mut Vec<u8>);
            if v.try_reserve(n as usize).is_err() {
                return 0;
            }
            v.extend_from_slice(std::slice::from_raw_parts(p, n as usize));
            1
        }
        let mut bytes: Vec<u8> = vec![];
        let mut writer = Writer {
            version: 1,
            write,
            sink: (&mut bytes as *mut Vec<u8>).cast(),
        };
        ok(
            unsafe { FPDF_SaveAsCopy(self.handle, &mut writer, 2) },
            "PDF 저장에 실패했습니다.",
        )?;
        Ok(bytes)
    }
    pub fn import(&self, other: &Self, indices: &[usize]) -> Result<()> {
        ensure!(
            !indices.is_empty() && indices.iter().all(|i| *i < other.count()),
            "페이지 범위를 확인하세요."
        );
        let pages: Vec<i32> = indices.iter().map(|&i| i as i32).collect();
        ok(
            unsafe {
                FPDF_ImportPagesByIndex(
                    self.handle,
                    other.handle,
                    pages.as_ptr(),
                    pages.len().try_into()?,
                    self.count() as i32,
                )
            },
            "페이지를 가져올 수 없습니다.",
        )
    }
    pub fn extract(&self, indices: &[usize]) -> Result<Vec<u8>> {
        ensure!(
            self.extractable(),
            "이 PDF는 추출 권한이 제한되어 있습니다."
        );
        let dst = Self::empty()?;
        dst.import(self, indices)?;
        dst.bytes()
    }
    pub fn rotate(&self, i: usize) -> Result<()> {
        let p = self.page(i)?;
        unsafe { FPDFPage_SetRotation(p.h, (FPDFPage_GetRotation(p.h) + 1) % 4) };
        Ok(())
    }
    pub fn delete(&self, i: usize) -> Result<()> {
        ensure!(
            self.count() > 1 && i < self.count(),
            "최소 한 페이지가 필요합니다."
        );
        unsafe { FPDFPage_Delete(self.handle, i as i32) };
        Ok(())
    }
    pub fn move_page(&self, from: usize, to: usize) -> Result<()> {
        ensure!(
            from < self.count() && to < self.count(),
            "페이지 범위를 확인하세요."
        );
        ok(
            unsafe { FPDF_MovePages(self.handle, &(from as i32), 1, to as i32) },
            "페이지를 이동할 수 없습니다.",
        )
    }
    pub fn images(paths: &[std::path::PathBuf]) -> Result<Vec<u8>> {
        ensure!(!paths.is_empty(), "이미지를 선택하세요.");
        let doc = Self::empty()?;
        for path in paths {
            let img = image::open(path)?;
            let sz = vec2(img.width() as f32, img.height() as f32) * 0.75;
            let page = doc.new_page(sz.x, sz.y)?;
            page.add_image(Rect::from_min_size(Pos2::ZERO, sz), &img)?;
        }
        doc.bytes()
    }
    fn destination(&self, dest: H, action: H) -> Option<Destination> {
        unsafe {
            let dst = if !dest.is_null() {
                dest
            } else if !action.is_null() && FPDFAction_GetType(action) == 1 {
                FPDFAction_GetDest(self.handle, action)
            } else {
                return None;
            };
            if dst.is_null() {
                return None;
            }
            let idx = usize::try_from(FPDFDest_GetDestPageIndex(self.handle, dst)).ok()?;
            if idx >= self.count() {
                return None;
            }
            let (mut hx, mut hy, mut hz, mut x, mut y, mut z) = (0, 0, 0, 0., 0., 0.);
            let point = if FPDFDest_GetLocationInPage(
                dst, &mut hx, &mut hy, &mut hz, &mut x, &mut y, &mut z,
            ) != 0
                && (hx != 0 || hy != 0)
            {
                self.page(idx).ok().map(|p| p.display(x, y))
            } else {
                None
            };
            Some(Destination { page: idx, point })
        }
    }
    pub fn bookmarks(&self) -> Vec<Bookmark> {
        unsafe {
            let mut stack = vec![(FPDFBookmark_GetFirstChild(self.handle, null_mut()), 0)];
            let mut seen = HashSet::new();
            let mut rows = vec![];
            while let Some((b, depth)) = stack.pop() {
                if b.is_null() || !seen.insert(b as usize) || seen.len() > 100_000 {
                    continue;
                }
                let title = utf16_value(|p, n| FPDFBookmark_GetTitle(b, p, n));
                let dest = self.destination(
                    FPDFBookmark_GetDest(self.handle, b),
                    FPDFBookmark_GetAction(b),
                );
                rows.push(Bookmark { title, dest, depth });
                stack.push((FPDFBookmark_GetNextSibling(self.handle, b), depth));
                if depth < 128 {
                    stack.push((FPDFBookmark_GetFirstChild(self.handle, b), depth + 1))
                }
            }
            rows
        }
    }
    pub fn label(&self, i: usize) -> String {
        unsafe { utf16_value(|p, n| FPDF_GetPageLabel(self.handle, i as i32, p, n)) }
    }
}

// Clamp before allocating: a PDF page can be much larger than the visible viewport.
pub fn raster_dimensions(size: Vec2, max_side: usize) -> [usize; 2] {
    let factor = (24_000_000. / (size.x * size.y))
        .sqrt()
        .min(max_side.min(16384) as f32 / size.max_elem())
        .min(1.);
    let pixels = (size * factor).round().max(Vec2::splat(1.));
    [pixels.x as usize, pixels.y as usize]
}

impl Page<'_> {
    fn device(&self) -> (i32, i32) {
        (
            (self.size.x * 64.).round().max(1.) as i32,
            (self.size.y * 64.).round().max(1.) as i32,
        )
    }
    pub fn display(&self, x: f32, y: f32) -> Pos2 {
        let (w, h) = self.device();
        let (mut dx, mut dy) = (0, 0);
        unsafe { FPDF_PageToDevice(self.h, 0, 0, w, h, 0, x as f64, y as f64, &mut dx, &mut dy) };
        pos2(dx as f32 / 64., dy as f32 / 64.)
    }
    pub fn native(&self, p: Pos2) -> Pos2 {
        let (w, h) = self.device();
        let (mut x, mut y) = (0., 0.);
        unsafe {
            FPDF_DeviceToPage(
                self.h,
                0,
                0,
                w,
                h,
                0,
                (p.x * 64.).round() as i32,
                (p.y * 64.).round() as i32,
                &mut x,
                &mut y,
            )
        };
        pos2(x as f32, y as f32)
    }
    pub fn render(&self, scale: f32) -> Result<RgbaImage> {
        self.render_bitmap(raster_dimensions(self.size * scale, 16384))
    }
    pub fn render_for_screen(&self, size: [usize; 2]) -> Result<RgbaImage> {
        let [width, height] = size;
        // At reading sizes sample fine glyphs, rules and embedded images before reducing
        // to the exact display grid. Large zoomed pages already have substantial detail.
        if width * height <= 4_000_000 && width.max(height) <= 8192 {
            let sampled = self.render_bitmap([width * 2, height * 2])?;
            let mut reduced = RgbaImage::new(width as u32, height as u32);
            // The page has already been composited onto an opaque white background.
            let filter = fast_image_resize::ResizeOptions::new().use_alpha(false);
            fast_image_resize::Resizer::new().resize(&sampled, &mut reduced, &filter)?;
            Ok(reduced)
        } else {
            self.render_bitmap(size)
        }
    }
    fn render_bitmap(&self, size: [usize; 2]) -> Result<RgbaImage> {
        self.bitmap_on(size.map(|n| n as u32), 255)
    }
    pub fn transparent_bitmap(&self, size: [u32; 2]) -> Result<RgbaImage> {
        self.bitmap_on(size, 0)
    }
    fn bitmap_on(&self, size: [u32; 2], background: u8) -> Result<RgbaImage> {
        let [width, height] = size;
        let (w, h) = (i32::try_from(width)?, i32::try_from(height)?);
        let stride = w.checked_mul(4).context("이미지 너비가 너무 큽니다.")?;
        ensure!(w > 0 && h > 0, "빈 이미지입니다.");
        let len = (stride as usize)
            .checked_mul(h as usize)
            .context("이미지 크기")?;
        let mut buf = Vec::new();
        buf.try_reserve_exact(len)
            .context("이미지 메모리를 할당할 수 없습니다.")?;
        buf.resize(len, background);
        let bmp = Bitmap(checked(
            unsafe { FPDFBitmap_CreateEx(w, h, 4, buf.as_mut_ptr(), stride) },
            "페이지 비트맵",
        )?);
        unsafe { FPDF_RenderPageBitmap(bmp.0, self.h, 0, 0, w, h, 0, 1 | 0x10) };
        drop(bmp);
        Ok(RgbaImage::from_raw(w as u32, h as u32, buf).unwrap())
    }
    pub fn layout(&self) -> Result<Layout> {
        unsafe {
            let text = Text(checked(FPDFText_LoadPage(self.h), "텍스트 읽기")?);
            let mut runs = vec![];
            let mut graphics = vec![];
            let mut rules = vec![];
            let mut glyphs = vec![];
            for i in 0..FPDFPage_CountObjects(self.h) {
                let obj = FPDFPage_GetObject(self.h, i);
                if obj.is_null() {
                    continue;
                }
                let (mut l, mut b, mut r, mut t) = (0., 0., 0., 0.);
                if FPDFPageObj_GetBounds(obj, &mut l, &mut b, &mut r, &mut t) == 0 {
                    continue;
                }
                let rect = Rect::from_two_pos(self.display(l, b), self.display(r, t));
                let kind = FPDFPageObj_GetType(obj);
                if kind != 1 {
                    graphics.push((i, rect, kind));
                    if kind == 2 {
                        let mut m = Matrix::default();
                        FPDFPageObj_GetMatrix(obj, &mut m);
                        let mut previous = None;
                        let mut first = None;
                        for j in 0..FPDFPath_CountSegments(obj) {
                            let seg = FPDFPath_GetPathSegment(obj, j);
                            let (mut x, mut y) = (0., 0.);
                            if seg.is_null() || FPDFPathSegment_GetPoint(seg, &mut x, &mut y) == 0 {
                                continue;
                            }
                            let pt = self.display(m.a * x + m.c * y + m.e, m.b * x + m.d * y + m.f);
                            match FPDFPathSegment_GetType(seg) {
                                2 => first = Some(pt),
                                0 => {
                                    if let Some(pr) = previous {
                                        rules.push((pr, pt));
                                    }
                                }
                                _ => {}
                            }
                            if FPDFPathSegment_GetClose(seg) != 0 {
                                if let Some(pr) = first {
                                    rules.push((pt, pr));
                                }
                            }
                            previous = Some(pt);
                        }
                    }
                    continue;
                }
                let value = utf16_value(|p, n| FPDFTextObj_GetText(obj, text.0, p, n));
                if value.is_empty() {
                    continue;
                }
                let font = FPDFTextObj_GetFont(obj);
                let mut buf = vec![0; FPDFFont_GetFamilyName(font, null_mut(), 0).min(4096)];
                if !buf.is_empty() {
                    FPDFFont_GetFamilyName(font, buf.as_mut_ptr(), buf.len());
                }
                let family = String::from_utf8_lossy(&buf)
                    .trim_end_matches('\0')
                    .to_owned();
                let mut m = Matrix::default();
                FPDFPageObj_GetMatrix(obj, &mut m);
                let mut sz = 12.;
                FPDFTextObj_GetFontSize(obj, &mut sz);
                let (mut red, mut green, mut blue, mut a) = (0, 0, 0, 255);
                FPDFPageObj_GetFillColor(obj, &mut red, &mut green, &mut blue, &mut a);
                let mut angle = 0;
                FPDFFont_GetItalicAngle(font, &mut angle);
                let clip = FPDFPageObj_GetClipPath(obj);
                let safe_overlay = FPDFPage_GetRotation(self.h) == 0
                    && m.a > 0.
                    && m.d > 0.
                    && m.b.abs() < 0.001
                    && m.c.abs() < 0.001
                    && a == 255
                    && FPDFTextObj_GetTextRenderMode(obj) == 0
                    && (clip.is_null() || FPDFClipPath_CountPaths(clip) <= 0);
                runs.push(Run {
                    index: i,
                    text: value,
                    rect,
                    origin: self.display(m.e, m.f),
                    matrix: m,
                    size: sz,
                    family,
                    rgb: [red as u8, green as u8, blue as u8],
                    bold: FPDFFont_GetWeight(font) >= 600,
                    italic: angle != 0,
                    safe_overlay,
                });
            }
            for i in 0..FPDFText_CountChars(text.0) {
                if let Some(ch) = char::from_u32(FPDFText_GetUnicode(text.0, i)) {
                    let (mut l, mut r, mut b, mut t) = (0., 0., 0., 0.);
                    let rect =
                        if FPDFText_GetCharBox(text.0, i, &mut l, &mut r, &mut b, &mut t) != 0 {
                            Rect::from_two_pos(
                                self.display(l as f32, b as f32),
                                self.display(r as f32, t as f32),
                            )
                        } else {
                            Rect::NOTHING
                        };
                    glyphs.push(Glyph { ch, rect });
                }
            }
            Ok(Layout {
                runs,
                graphics,
                rules,
                glyphs,
            })
        }
    }
    pub fn link(&self, pos: Pos2) -> Option<Destination> {
        let p = self.native(pos);
        unsafe {
            let l = FPDFLink_GetLinkAtPoint(self.h, p.x as f64, p.y as f64);
            if l.is_null() {
                None
            } else {
                self.doc
                    .destination(FPDFLink_GetDest(self.doc.handle, l), FPDFLink_GetAction(l))
            }
        }
    }
    pub fn set_active(&self, index: i32, active: bool) -> Result<()> {
        let obj = checked(unsafe { FPDFPage_GetObject(self.h, index) }, "페이지 개체")?;
        ok(
            unsafe { FPDFPageObj_SetIsActive(obj, active as i32) },
            "표시 상태를 변경할 수 없습니다.",
        )
    }
    fn generate(&self) -> Result<()> {
        ok(
            unsafe { FPDFPage_GenerateContent(self.h) },
            "PDF 페이지 생성을 실패했습니다.",
        )
    }
    pub fn font_bytes(&self, index: i32) -> Option<Vec<u8>> {
        unsafe {
            let obj = FPDFPage_GetObject(self.h, index);
            if obj.is_null() {
                return None;
            }
            let font = FPDFTextObj_GetFont(obj);
            let mut n = 0;
            if FPDFFont_GetFontData(font, null_mut(), 0, &mut n) == 0
                || n == 0
                || n > 64 * 1024 * 1024
            {
                return None;
            }
            let mut out = vec![0; n];
            if FPDFFont_GetFontData(font, out.as_mut_ptr(), out.len(), &mut n) == 0 {
                return None;
            }
            Some(out)
        }
    }
    fn load_font(&self, path: &Path) -> Result<Font> {
        let data = std::fs::read(path).with_context(|| format!("글꼴 {}", path.display()))?;
        Ok(Font(checked(
            unsafe {
                FPDFText_LoadFont(self.doc.handle, data.as_ptr(), data.len().try_into()?, 2, 1)
            },
            "글꼴 불러오기",
        )?))
    }
    fn insert_text(
        &self,
        text: &str,
        style: &Style,
        font: &Font,
        m: Matrix,
        at: Option<usize>,
    ) -> Result<()> {
        let mut obj = NewObject(checked(
            unsafe { FPDFPageObj_CreateTextObj(self.doc.handle, font.0, style.size) },
            "텍스트 만들기",
        )?);
        unsafe {
            ok(
                FPDFText_SetText(obj.0, wide(text).as_ptr()),
                "글자 인코딩 실패",
            )?;
            let [r, g, b] = style.rgb;
            ok(
                FPDFPageObj_SetFillColor(obj.0, r.into(), g.into(), b.into(), 255),
                "텍스트 색상 변경 실패",
            )?;
            ok(FPDFPageObj_SetMatrix(obj.0, &m), "텍스트 위치 변경 실패")?;
            if let Some(i) = at {
                ok(
                    FPDFPage_InsertObjectAtIndex(self.h, obj.0, i),
                    "텍스트 삽입 실패",
                )?
            } else {
                FPDFPage_InsertObject(self.h, obj.0)
            };
            obj.0 = null_mut();
        }
        Ok(())
    }
    pub fn edit(&self, edit: &Edit) -> Result<()> {
        ensure!(
            edit.style.size.is_finite() && edit.style.size > 0.,
            "글자 크기를 확인하세요."
        );
        if edit.runs.is_empty() {
            let font = self.load_font(&edit.style.font.clone().unwrap_or_else(default_font))?;
            let line_h = edit.style.size * 1.22;
            for (i, line) in edit.text.lines().enumerate() {
                let baseline = edit.rect.top() + edit.style.size + i as f32 * line_h;
                ensure!(baseline <= self.size.y + 0.5, "텍스트가 페이지를 넘습니다.");
                let p = self.native(pos2(edit.rect.left(), baseline));
                let right = self.native(pos2(edit.rect.left() + 1., baseline));
                let up = self.native(pos2(edit.rect.left(), baseline - 1.));
                self.insert_text(
                    line,
                    &edit.style,
                    &font,
                    Matrix {
                        a: right.x - p.x,
                        b: right.y - p.y,
                        c: up.x - p.x,
                        d: up.y - p.y,
                        e: p.x,
                        f: p.y,
                    },
                    None,
                )?;
            }
        } else {
            let values = split_edit(&edit.runs, &edit.text);
            let mut pairs: Vec<_> = edit
                .runs
                .iter()
                .zip(values)
                .map(|(run, v)| {
                    Ok((
                        run,
                        v,
                        checked(
                            unsafe { FPDFPage_GetObject(self.h, run.index) },
                            "편집 개체",
                        )?,
                    ))
                })
                .collect::<Result<_>>()?;
            let mut shift = 0.;
            let mut shift_indices: Vec<i32> = vec![];
            for (run, value, obj) in pairs.drain(..) {
                let index =
                    run.index - shift_indices.iter().filter(|idx| **idx < run.index).count() as i32;
                let size = run.size * edit.style.size / edit.runs[0].size.max(0.01);
                let rgb = if edit.style.rgb == edit.runs[0].rgb {
                    run.rgb
                } else {
                    edit.style.rgb
                };
                let advance = run.rect.right() - run.origin.x;
                if value.is_empty() {
                    unsafe {
                        ok(FPDFPage_RemoveObject(self.h, obj), "문구 삭제 실패")?;
                        FPDFPageObj_Destroy(obj)
                    };
                    shift_indices.push(run.index);
                    shift -= advance;
                    continue;
                }
                let mut matrix = run.matrix;
                let origin = self.native(run.origin + vec2(shift, 0.));
                matrix.e = origin.x;
                matrix.f = origin.y;
                let mut replacement = edit.style.font.clone();
                if replacement.is_none() && value != run.text {
                    unsafe {
                        let changed = FPDFText_SetText(obj, wide(&value).as_ptr()) != 0;
                        let t = Text(checked(FPDFText_LoadPage(self.h), "문자 검사")?);
                        let actual = utf16_value(|p, n| FPDFTextObj_GetText(obj, t.0, p, n));
                        if !changed || actual.replace('\u{a0}', " ") != value.replace('\u{a0}', " ")
                        {
                            replacement = Some(default_font());
                        }
                    }
                }
                let new_obj = if let Some(path) = replacement {
                    let font = self.load_font(&path)?;
                    let style = Style {
                        size,
                        rgb,
                        font: None,
                    };
                    unsafe {
                        ok(FPDFPage_RemoveObject(self.h, obj), "문구 교체 실패")?;
                        FPDFPageObj_Destroy(obj)
                    };
                    self.insert_text(&value, &style, &font, matrix, Some(index as usize))?;
                    checked(unsafe { FPDFPage_GetObject(self.h, index) }, "새 문구")?
                } else {
                    unsafe {
                        if size != run.size {
                            ok(FPDFTextObj_SetFontSize(obj, size), "글자 크기 변경 실패")?;
                        }
                        if rgb != run.rgb {
                            ok(
                                FPDFPageObj_SetFillColor(
                                    obj,
                                    rgb[0].into(),
                                    rgb[1].into(),
                                    rgb[2].into(),
                                    255,
                                ),
                                "색상 변경 실패",
                            )?;
                        }
                        if shift.abs() > 0.001 {
                            ok(FPDFPageObj_SetMatrix(obj, &matrix), "문구 위치 변경 실패")?;
                        }
                        obj
                    }
                };
                if run.matrix.b.abs() < 0.001 && run.matrix.c.abs() < 0.001 {
                    let (mut l, mut b, mut r, mut t) = (0., 0., 0., 0.);
                    unsafe { FPDFPageObj_GetBounds(new_obj, &mut l, &mut b, &mut r, &mut t) };
                    let rect = Rect::from_two_pos(self.display(l, b), self.display(r, t));
                    shift += rect.right() - (run.origin.x + shift) - advance;
                    ensure!(
                        rect.right() <= self.size.x.max(run.rect.right()) + 1.,
                        "텍스트가 페이지 폭을 넘습니다."
                    );
                }
            }
        }
        self.generate()
    }
    pub fn add_image(&self, rect: Rect, image: &DynamicImage) -> Result<()> {
        let mut img = image.to_rgba8();
        let (w, h) = img.dimensions();
        ensure!(
            w > 0 && h > 0 && w <= i32::MAX as u32 / 4,
            "이미지 크기를 확인하세요."
        );
        for p in img.pixels_mut() {
            p.0.swap(0, 2)
        }
        let o = self.native(rect.left_bottom());
        let r = self.native(rect.right_bottom());
        let t = self.native(rect.left_top());
        let m = Matrix {
            a: r.x - o.x,
            b: r.y - o.y,
            c: t.x - o.x,
            d: t.y - o.y,
            e: o.x,
            f: o.y,
        };
        unsafe {
            let bmp = Bitmap(checked(
                FPDFBitmap_CreateEx(w as i32, h as i32, 4, img.as_mut_ptr(), w as i32 * 4),
                "이미지 비트맵",
            )?);
            let mut obj = NewObject(checked(
                FPDFPageObj_NewImageObj(self.doc.handle),
                "이미지 개체",
            )?);
            let mut page = self.h;
            ok(
                FPDFImageObj_SetBitmap(&mut page, 1, obj.0, bmp.0),
                "이미지를 불러올 수 없습니다.",
            )?;
            ok(FPDFPageObj_SetMatrix(obj.0, &m), "이미지 위치 설정 실패")?;
            FPDFPage_InsertObject(self.h, obj.0);
            obj.0 = null_mut();
        }
        self.generate()
    }
}
pub fn default_font() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
        .join("Fonts/malgun.ttf")
}
fn split_edit(runs: &[Run], new: &str) -> Vec<String> {
    let old: Vec<char> = runs.iter().flat_map(|r| r.text.chars()).collect();
    let new: Vec<char> = new.chars().collect();
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut pos = 0;
    let mut out = vec![];
    let mut inserted = false;
    for (i, r) in runs.iter().enumerate() {
        let end = pos + r.text.chars().count();
        let mut s = String::new();
        if pos < head {
            s.extend(&old[pos..end.min(head)])
        }
        if !inserted && (head < end || i + 1 == runs.len()) {
            s.extend(&new[head..new.len() - tail]);
            inserted = true
        }
        if end > old.len() - tail {
            s.extend(&old[pos.max(old.len() - tail)..end])
        }
        out.push(s);
        pos = end;
    }
    out
}
pub fn save_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut f = tempfile::NamedTempFile::new_in(parent)?;
    f.write_all(bytes)?;
    f.as_file().sync_all()?;
    f.persist(path).map_err(|e| anyhow!(e.error))?;
    Ok(())
}
