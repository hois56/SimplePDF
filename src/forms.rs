//! Expose text inside ordinary Form XObjects on a private copy of one page.
//! Resources and content streams may be shared by other pages or placements.
use anyhow::{ensure, Context, Result};
use lopdf::{
    content::{Content, Operation},
    Dictionary, Document, Object, ObjectId, Stream,
};
use std::collections::{HashMap, HashSet};

fn dict<'a>(d: &'a Document, v: &'a Object) -> Option<&'a Dictionary> {
    d.dereference(v).ok()?.1.as_dict().ok()
}
fn resources(d: &Document, mut page: ObjectId) -> Result<Dictionary> {
    let mut seen = HashSet::new();
    while seen.insert(page) {
        let node = d.get_dictionary(page)?;
        if let Ok(obj) = node.get(b"Resources") {
            return Ok(dict(d, obj).context("PDF 리소스")?.clone());
        }
        if let Ok(p) = node.get(b"Parent").and_then(Object::as_reference) {
            page = p
        } else {
            break;
        }
    }
    Ok(Dictionary::new())
}
fn kind(op: &str) -> Option<(&'static [u8], usize)> {
    match op {
        "Tf" => Some((b"Font", 0)),
        "Do" => Some((b"XObject", 0)),
        "gs" => Some((b"ExtGState", 0)),
        "cs" | "CS" => Some((b"ColorSpace", 0)),
        "sh" => Some((b"Shading", 0)),
        "BDC" | "DP" => Some((b"Properties", 1)),
        _ => None,
    }
}
struct Expansion<'a> {
    doc: &'a Document,
    resources: Dictionary,
    sequence: usize,
    operations: usize,
}
impl Expansion<'_> {
    fn expand(
        &mut self,
        ops: Vec<Operation>,
        scope: &Dictionary,
        depth: usize,
    ) -> Result<Vec<Operation>> {
        ensure!(depth < 32, "PDF 양식 중첩이 너무 깊습니다.");
        let mut out = vec![];
        let mut names = HashMap::new();
        for mut op in ops {
            self.operations += 1;
            ensure!(
                self.operations <= 1_000_000,
                "PDF 페이지 개체 수가 너무 많습니다."
            );
            // lopdf's encoder does not round trip inline image data as an ordinary operator.
            ensure!(
                op.operator != "BI" && op.operator != "ID",
                "인라인 이미지가 있는 양식은 원본 형식으로 유지합니다."
            );
            if op.operator == "Do" {
                if let Some(name) = op.operands.first().and_then(|v| v.as_name().ok()) {
                    let form = scope
                        .get(b"XObject")
                        .ok()
                        .and_then(|v| dict(self.doc, v))
                        .and_then(|v| v.get(name).ok())
                        .and_then(|v| self.doc.dereference(v).ok())
                        .and_then(|(_, v)| v.as_stream().ok());
                    if let Some(f) = form.filter(|f| {
                        f.dict.get(b"Subtype").and_then(Object::as_name).ok()
                            == Some(b"Form".as_slice())
                            && ![
                                b"Group".as_slice(),
                                b"OC",
                                b"Ref",
                                b"StructParent",
                                b"StructParents",
                            ]
                            .iter()
                            .any(|k| f.dict.has(k))
                    }) {
                        let coords = f.dict.get(b"BBox")?.as_array()?;
                        ensure!(coords.len() == 4, "양식 경계");
                        let v = coords
                            .iter()
                            .map(Object::as_float)
                            .collect::<lopdf::Result<Vec<_>>>()?;
                        let sub = f
                            .dict
                            .get(b"Resources")
                            .ok()
                            .and_then(|v| dict(self.doc, v))
                            .unwrap_or(scope);
                        let content = Content::decode(&f.get_plain_content()?)?;
                        self.sequence += 1;
                        out.push(Operation::new("q", vec![]));
                        if let Ok(m) = f.dict.get(b"Matrix") {
                            out.push(Operation::new("cm", m.as_array()?.clone()))
                        }
                        out.push(Operation::new(
                            "re",
                            vec![
                                v[0].into(),
                                v[1].into(),
                                (v[2] - v[0]).into(),
                                (v[3] - v[1]).into(),
                            ],
                        ));
                        out.push(Operation::new("W", vec![]));
                        out.push(Operation::new("n", vec![]));
                        out.extend(self.expand(content.operations, sub, depth + 1)?);
                        out.push(Operation::new("Q", vec![]));
                        continue;
                    }
                }
            }
            let slot = kind(&op.operator).or_else(|| {
                matches!(op.operator.as_str(), "scn" | "SCN")
                    .then(|| (b"Pattern".as_slice(), op.operands.len().saturating_sub(1)))
            });
            if depth > 0 {
                if let Some((group, idx)) = slot {
                    if let Some(name) = op.operands.get(idx).and_then(|v| v.as_name().ok()) {
                        if let Some(value) = scope
                            .get(group)
                            .ok()
                            .and_then(|v| dict(self.doc, v))
                            .and_then(|d| d.get(name).ok())
                        {
                            let key = (group.to_vec(), name.to_vec());
                            let new = names.entry(key).or_insert_with(|| {
                                self.sequence += 1;
                                let mut name =
                                    format!("SimplePDFForm{}", self.sequence).into_bytes();
                                let mut all = self
                                    .resources
                                    .get(group)
                                    .ok()
                                    .and_then(|v| dict(self.doc, v))
                                    .cloned()
                                    .unwrap_or_default();
                                while all.has(&name) {
                                    name.push(b'_')
                                }
                                all.set(name.clone(), value.clone());
                                self.resources.set(group, all);
                                name
                            });
                            op.operands[idx] = Object::Name(new.clone());
                        }
                    }
                }
            }
            out.push(op);
        }
        Ok(out)
    }
}
pub fn page_copy(bytes: &[u8], password: &str, number: usize) -> Result<Vec<u8>> {
    let mut d = Document::load_mem(bytes)?;
    if d.is_encrypted() {
        d.decrypt(password)?;
    }
    let id = *d.get_pages().get(&(number as u32 + 1)).context("페이지")?;
    let res = resources(&d, id)?;
    let mut ex = Expansion {
        doc: &d,
        resources: res.clone(),
        sequence: 0,
        operations: 0,
    };
    let ops = ex.expand(
        Content::decode(&d.get_page_content(id)?)?.operations,
        &res,
        0,
    )?;
    ensure!(ex.sequence > 0, "편집 가능한 양식 개체가 없습니다.");
    let resources = ex.resources;
    let stream = Stream::new(Dictionary::new(), Content { operations: ops }.encode()?);
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
