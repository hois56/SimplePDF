//! Isolate one image placement, including its enclosing Form XObjects, before replacement.
use crate::{forms, pdf::Page};
use anyhow::{ensure, Context, Result};
use eframe::egui::{pos2, Pos2, Rect, Vec2};
use image::{DynamicImage, GenericImageView};
use lopdf::{
    content::{Content, Operation},
    dictionary, Dictionary, Document, Object, ObjectId, Stream,
};

#[derive(Clone)]
pub struct Picture {
    path: Vec<usize>,
    pub corners: [Pos2; 4],
    pub pixels: [u32; 2],
    clip: Rect,
}
impl Picture {
    pub fn contains(&self, point: Pos2) -> bool {
        let [a, b, _, d] = self.corners;
        let (u, v, p) = (b - a, d - a, point - a);
        let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
        let det = cross(u, v);
        self.clip.contains(point)
            && det.abs() > 0.0001
            && (0.0..=1.0).contains(&(cross(p, v) / det))
            && (0.0..=1.0).contains(&(cross(u, p) / det))
    }
    pub fn bounds(&self) -> Rect {
        Rect::from_points(&self.corners).intersect(self.clip)
    }
    fn size(&self) -> Vec2 {
        Vec2::new(
            self.corners[0].distance(self.corners[1]),
            self.corners[0].distance(self.corners[3]),
        )
    }
}

pub fn fill_frame(image: &DynamicImage, size: Vec2) -> DynamicImage {
    let (w, h) = image.dimensions();
    let ratio = f64::from(size.x) / f64::from(size.y);
    let cw = (h as f64 * ratio).round().clamp(1., w as f64) as u32;
    let ch = (w as f64 / ratio).round().clamp(1., h as f64) as u32;
    image.crop_imm((w - cw) / 2, (h - ch) / 2, cw, ch)
}

#[derive(Clone, Copy)]
struct Transform([f32; 6]);
impl Transform {
    const ID: Self = Self([1., 0., 0., 1., 0., 0.]);
    fn read(args: &[Object]) -> Result<Self> {
        ensure!(args.len() == 6, "이미지 변환 행렬");
        let mut v = [0.; 6];
        for (n, item) in v.iter_mut().zip(args) {
            *n = item.as_float()?;
        }
        Ok(Self(v))
    }
    fn point(self, x: f32, y: f32) -> Pos2 {
        let [a, b, c, d, e, f] = self.0;
        pos2(a * x + c * y + e, b * x + d * y + f)
    }
    fn then(self, child: Self) -> Self {
        let [a, b, c, d, e, f] = child.0;
        let origin = self.point(e, f);
        let x = self.point(e + a, f + b) - origin;
        let y = self.point(e + c, f + d) - origin;
        Self([x.x, x.y, y.x, y.y, origin.x, origin.y])
    }
    fn corners(self, page: &Page<'_>, x: f32, y: f32, w: f32, h: f32) -> [Pos2; 4] {
        [(x, y), (x + w, y), (x + w, y + h), (x, y + h)].map(|(x, y)| {
            let pt = self.point(x, y);
            page.display(pt.x, pt.y)
        })
    }
}

fn operations(mut data: Vec<u8>) -> Result<Vec<Operation>> {
    // The parser can return a valid prefix. Never rewrite a truncated stream.
    data.extend_from_slice(b"\nSimplePDFEnd\n");
    let mut ops = std::panic::catch_unwind(|| Content::decode(&data))
        .map_err(|_| anyhow::anyhow!("인라인 이미지 형식을 읽을 수 없습니다."))??
        .operations;
    ensure!(
        ops.pop().is_some_and(|op| op.operator == "SimplePDFEnd"),
        "이 PDF의 이미지 명령을 읽을 수 없습니다."
    );
    Ok(ops)
}
fn encoded(ops: &[Operation]) -> Result<Vec<u8>> {
    if ops.iter().all(|op| op.operator != "BI") {
        return Ok(Content { operations: ops }.encode()?);
    }
    // lopdf represents BI's dictionary and binary payload as a stream operand.
    let mut bytes = vec![];
    for op in ops {
        if op.operator == "BI" {
            let image = op.operands.first().context("인라인 이미지")?.as_stream()?;
            bytes.extend_from_slice(b"BI\n");
            for (key, value) in &image.dict {
                if key == b"Length" {
                    continue;
                }
                let pair = Operation::new("", vec![Object::Name(key.clone()), value.clone()]);
                bytes.extend(Content { operations: [pair] }.encode()?);
            }
            bytes.extend_from_slice(b"ID\n");
            bytes.extend_from_slice(&image.content);
            bytes.extend_from_slice(b"\nEI\n");
        } else {
            bytes.extend(
                Content {
                    operations: std::slice::from_ref(op),
                }
                .encode()?,
            );
            bytes.push(b'\n');
        }
    }
    Ok(bytes)
}
pub fn open(data: &[u8], password: &str) -> Result<Document> {
    let mut d = Document::load_mem(data)?;
    if d.is_encrypted() {
        d.decrypt(password)?
    }
    Ok(d)
}
fn target<'a>(d: &'a Document, scope: &'a Dictionary, op: &Operation) -> Option<&'a Stream> {
    let name = op.operands.first()?.as_name().ok()?;
    let x = scope.get(b"XObject").ok().and_then(|v| forms::dict(d, v))?;
    d.dereference(x.get(name).ok()?).ok()?.1.as_stream().ok()
}
fn dimensions(d: &Document, image: &Stream) -> Result<[u32; 2]> {
    let dimension = |key: &[u8], abbr: &[u8]| -> Result<u32> {
        let v = image.dict.get(key).or_else(|_| image.dict.get(abbr))?;
        let n = u32::try_from(d.dereference(v)?.1.as_i64()?)?;
        ensure!(n > 0, "이미지 크기를 읽을 수 없습니다.");
        Ok(n)
    };
    Ok([dimension(b"Width", b"W")?, dimension(b"Height", b"H")?])
}
fn scope<'a>(d: &'a Document, form: &'a Stream, parent: &'a Dictionary) -> &'a Dictionary {
    form.dict
        .get(b"Resources")
        .ok()
        .and_then(|v| forms::dict(d, v))
        .unwrap_or(parent)
}
struct Scan<'a, 'p> {
    doc: &'a Document,
    page: &'a Page<'p>,
    pictures: Vec<Picture>,
    visited: usize,
}
impl Scan<'_, '_> {
    fn walk(
        &mut self,
        ops: &[Operation],
        resources: &Dictionary,
        mut matrix: Transform,
        mut clip: Rect,
        path: &mut Vec<usize>,
    ) -> Result<()> {
        ensure!(path.len() < 48, "PDF 양식이 너무 깊게 중첩되어 있습니다.");
        let mut stack = vec![];
        let mut path_bounds = Rect::NOTHING;
        let mut clipping = false;
        for (i, op) in ops.iter().enumerate() {
            self.visited += 1;
            ensure!(
                self.visited <= 2_000_000,
                "페이지의 이미지 명령이 너무 많습니다."
            );
            let args = &op.operands;
            match op.operator.as_str() {
                "q" => stack.push((matrix, clip)),
                "Q" => {
                    if let Some(state) = stack.pop() {
                        (matrix, clip) = state
                    }
                }
                "cm" => matrix = matrix.then(Transform::read(args)?),
                "re" if args.len() == 4 => {
                    let q = matrix.corners(
                        self.page,
                        args[0].as_float()?,
                        args[1].as_float()?,
                        args[2].as_float()?,
                        args[3].as_float()?,
                    );
                    path_bounds = path_bounds.union(Rect::from_points(&q));
                }
                "m" | "l" | "c" | "v" | "y" => {
                    for pair in args.chunks_exact(2) {
                        let p = matrix.point(pair[0].as_float()?, pair[1].as_float()?);
                        path_bounds.extend_with(self.page.display(p.x, p.y));
                    }
                }
                "W" | "W*" => clipping = true,
                "n" | "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                    if clipping {
                        clip = clip.intersect(path_bounds)
                    }
                    path_bounds = Rect::NOTHING;
                    clipping = false;
                }
                "Do" | "BI" => {
                    let source = target(self.doc, resources, op);
                    let kind = source
                        .and_then(|s| s.dict.get(b"Subtype").ok())
                        .and_then(|v| v.as_name().ok());
                    path.push(i);
                    if op.operator == "BI" || kind == Some(b"Image") {
                        let image = source
                            .or_else(|| op.operands.first()?.as_stream().ok())
                            .context("이미지 정보")?;
                        let pixels = dimensions(self.doc, image)?;
                        let corners = matrix.corners(self.page, 0., 0., 1., 1.);
                        let pic = Picture {
                            pixels,
                            path: path.clone(),
                            corners,
                            clip,
                        };
                        if pic.bounds().is_positive() {
                            self.pictures.push(pic)
                        }
                    } else if let Some(form) = source.filter(|_| kind == Some(b"Form")) {
                        let mut m = matrix;
                        if let Ok(v) = form.dict.get(b"Matrix") {
                            m = m.then(Transform::read(v.as_array()?)?)
                        }
                        let mut bounds = clip;
                        if let Ok(v) = form.dict.get(b"BBox").and_then(Object::as_array) {
                            if let [l, b, r, t] = v.as_slice() {
                                let (x, y) = (l.as_float()?, b.as_float()?);
                                bounds = clip.intersect(Rect::from_points(&m.corners(
                                    self.page,
                                    x,
                                    y,
                                    r.as_float()? - x,
                                    t.as_float()? - y,
                                )));
                            }
                        }
                        self.walk(
                            &operations(form.get_plain_content()?)?,
                            scope(self.doc, form, resources),
                            m,
                            bounds,
                            path,
                        )?;
                    }
                    path.pop();
                }
                _ => {}
            }
        }
        Ok(())
    }
}
pub fn placements(d: &Document, number: usize, page: &Page<'_>) -> Result<Vec<Picture>> {
    let id = *d.get_pages().get(&(number as u32 + 1)).context("페이지")?;
    let mut scan = Scan {
        doc: d,
        page,
        pictures: vec![],
        visited: 0,
    };
    scan.walk(
        &operations(d.get_page_content(id)?)?,
        &forms::resources(d, id)?,
        Transform::ID,
        Rect::from_min_size(Pos2::ZERO, page.size),
        &mut vec![],
    )?;
    Ok(scan.pictures)
}

// Draw just this image with its own resources in the original pixel orientation.
fn isolated_image(mut d: Document, number: usize, picture: &Picture) -> Result<Vec<u8>> {
    let id = *d.get_pages().get(&(number as u32 + 1)).context("페이지")?;
    let mut resources = forms::resources(&d, id)?;
    let mut ops = operations(d.get_page_content(id)?)?;
    let (last, parents) = picture.path.split_last().context("선택한 이미지")?;
    for &index in parents {
        let op = ops.get(index).context("이미지 양식 위치")?;
        let form = target(&d, &resources, op).context("이미지 양식")?;
        ops = operations(form.get_plain_content()?)?;
        resources = scope(&d, form, &resources).clone();
    }
    let op = ops.get(*last).context("이미지 위치")?;
    let mut local = Dictionary::new();
    if let Ok(colors) = resources.get(b"ColorSpace") {
        local.set("ColorSpace", colors.clone());
    }
    if op.operator == "Do" {
        let name = op.operands.first().context("이미지 이름")?.as_name()?;
        let table = forms::dict(&d, resources.get(b"XObject")?).context("이미지 리소스")?;
        let value = table.get(name)?.clone();
        let source = target(&d, &resources, op).context("이미지")?;
        ensure!(
            dimensions(&d, source)? == picture.pixels,
            "이미지가 변경되었습니다."
        );
        let mut x = Dictionary::new();
        x.set(name.to_vec(), value);
        local.set("XObject", x);
    } else {
        ensure!(op.operator == "BI", "이미지 위치가 변경되었습니다.");
    }
    let [w, h] = picture.pixels.map(i64::from);
    let mut bytes = format!("q {w} 0 0 {h} 0 0 cm\n").into_bytes();
    bytes.extend(encoded(std::slice::from_ref(op))?);
    bytes.extend_from_slice(b"\nQ");
    let contents = d.add_object(Stream::new(Dictionary::new(), bytes));
    let pages = d.new_object_id();
    let page=d.add_object(dictionary!{"Type"=>"Page","Parent"=>pages,"MediaBox"=>vec![0.into(),0.into(),w.into(),h.into()],
        "Resources"=>local,"Contents"=>contents});
    d.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages","Count"=>1,"Kids"=>vec![page.into()]}.into(),
    );
    let catalog = d.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages});
    d.trailer = dictionary! {"Root"=>catalog};
    d.prune_objects();
    let mut out = Vec::new();
    d.save_to(&mut out)?;
    Ok(out)
}

pub fn export_png(
    source: Document,
    page: usize,
    picture: &Picture,
    path: &std::path::Path,
) -> Result<()> {
    let bytes = isolated_image(source, page, picture)?;
    let isolated = crate::pdf::Document::open(bytes, "")?;
    let rgba = isolated.page(0)?.transparent_bitmap(picture.pixels)?;
    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png)?;
    crate::pdf::save_atomic(path, png.get_ref())
}

fn pixels(d: &mut Document, img: DynamicImage) -> Result<ObjectId> {
    let (w, h) = img.dimensions();
    let mut desc = dictionary! {"Type"=>"XObject","Subtype"=>"Image","Width"=>i64::from(w),"Height"=>i64::from(h),"BitsPerComponent"=>8,"ColorSpace"=>"DeviceRGB"};
    if img.color().has_alpha() {
        let alpha: Vec<_> = img.to_rgba8().pixels().map(|p| p[3]).collect();
        if alpha.iter().any(|a| *a != 255) {
            let mut gray = desc.clone();
            gray.set("ColorSpace", "DeviceGray");
            let mut mask = Stream::new(gray, alpha);
            mask.compress()?;
            desc.set("SMask", d.add_object(mask));
        }
    }
    let mut stream = Stream::new(desc, img.into_rgb8().into_raw());
    stream.compress()?;
    Ok(d.add_object(stream))
}
fn rewrite(
    d: &mut Document,
    data: Vec<u8>,
    mut resources: Dictionary,
    path: &[usize],
    image: ObjectId,
) -> Result<(Vec<u8>, Dictionary)> {
    let mut ops = operations(data)?;
    let (i, tail) = path.split_first().context("선택한 이미지")?;
    let op = ops.get_mut(*i).context("이미지 위치")?;
    let child = if tail.is_empty() {
        ensure!(
            op.operator == "BI"
                || target(d, &resources, op).is_some_and(|s| s
                    .dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .ok()
                    == Some(b"Image")),
            "이미지 위치가 변경되었습니다."
        );
        image
    } else {
        let mut form = target(d, &resources, op).context("이미지 양식")?.clone();
        let local = scope(d, &form, &resources).clone();
        let (bytes, local) = rewrite(d, form.get_plain_content()?, local, tail, image)?;
        form.set_plain_content(bytes);
        form.dict.set("Resources", local);
        form.compress()?;
        d.add_object(form)
    };
    let mut x = resources
        .get(b"XObject")
        .ok()
        .and_then(|v| forms::dict(d, v))
        .cloned()
        .unwrap_or_default();
    let mut name = format!("SimplePDFImage{}", child.0).into_bytes();
    while x.has(&name) {
        name.push(b'_')
    }
    x.set(name.clone(), child);
    resources.set("XObject", x);
    *op = Operation::new("Do", vec![Object::Name(name)]);
    Ok((encoded(&ops)?, resources))
}
pub fn replace(
    mut d: Document,
    number: usize,
    placement: &Picture,
    image: &DynamicImage,
) -> Result<Vec<u8>> {
    let id = *d.get_pages().get(&(number as u32 + 1)).context("페이지")?;
    let data = d.get_page_content(id)?;
    let resources = forms::resources(&d, id)?;
    let image = pixels(&mut d, fill_frame(image, placement.size()))?;
    let (data, resources) = rewrite(&mut d, data, resources, &placement.path, image)?;
    let mut stream = Stream::new(Dictionary::new(), data);
    stream.compress()?;
    let contents = d.add_object(stream);
    let page = d.get_dictionary_mut(id)?;
    page.set("Contents", contents);
    page.set("Resources", resources);
    if let Some(state) = d.encryption_state.clone() {
        d.encrypt(&state)?
    }
    let mut out = vec![];
    d.save_to(&mut out)?;
    Ok(out)
}
