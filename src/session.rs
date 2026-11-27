use crate::pdf::{save_atomic, Bookmark, Destination, Document, Glyph, Layout, Run};
use anyhow::{ensure, Context, Result};
use eframe::egui::{Pos2, Rect};
use lopdf::{dictionary, Dictionary, Object, ObjectId};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone)]
struct Snapshot {
    data: Arc<Vec<u8>>,
    id: u64,
}
pub struct Session {
    pub doc: Document,
    pub path: Option<PathBuf>,
    pub password: String,
    pub bookmarks: Vec<Bookmark>,
    current: Snapshot,
    undo: VecDeque<Snapshot>,
    redo: Vec<Snapshot>,
    saved: Option<u64>,
    next: u64,
}
impl Session {
    pub fn open(data: Vec<u8>, path: Option<PathBuf>, password: String) -> Result<Self> {
        let doc = Document::open(data.clone(), &password)?;
        let bookmarks = doc.bookmarks();
        let saved = path.is_some().then_some(0);
        Ok(Self {
            doc,
            path,
            password,
            bookmarks,
            current: Snapshot {
                data: Arc::new(data),
                id: 0,
            },
            undo: Default::default(),
            redo: vec![],
            saved,
            next: 1,
        })
    }
    pub fn dirty(&self) -> bool {
        self.saved != Some(self.current.id)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.current.data
    }
    pub fn apply(&mut self, modify: impl FnOnce(&Document) -> Result<()>) -> Result<()> {
        ensure!(
            self.doc.editable(),
            "이 PDF는 편집 권한이 제한되어 있습니다."
        );
        let work = Document::open(self.bytes().to_vec(), &self.password)?;
        modify(&work)?;
        self.replace(work.bytes()?)
    }
    pub fn edit(&mut self, edit: &crate::pdf::Edit, base: Option<&[u8]>) -> Result<()> {
        ensure!(
            self.doc.editable(),
            "이 PDF는 편집 권한이 제한되어 있습니다."
        );
        let d = Document::open(
            base.unwrap_or_else(|| self.bytes()).to_vec(),
            &self.password,
        )?;
        d.page(edit.page)?.edit(edit)?;
        self.replace(d.bytes()?)
    }
    pub fn export_picture(
        &self,
        page: usize,
        picture: &crate::pictures::Picture,
        path: &Path,
    ) -> Result<()> {
        ensure!(
            self.doc.extractable(),
            "이 PDF는 이미지 추출 권한이 제한되어 있습니다."
        );
        crate::pictures::export_png(
            crate::pictures::open(self.bytes(), &self.password)?,
            page,
            picture,
            path,
        )
    }
    pub fn replace_picture(
        &mut self,
        number: usize,
        picture: &crate::pictures::Picture,
        image: &image::DynamicImage,
    ) -> Result<()> {
        ensure!(
            self.doc.editable(),
            "이 PDF는 편집 권한이 제한되어 있습니다."
        );
        let work = crate::pictures::open(self.bytes(), &self.password)?;
        self.replace(crate::pictures::replace(work, number, picture, image)?)
    }
    fn replace(&mut self, data: Vec<u8>) -> Result<()> {
        let doc = Document::open(data.clone(), &self.password)?;
        ensure!(
            !self.doc.encrypted() || doc.encrypted(),
            "원본 PDF 암호를 유지할 수 없습니다."
        );
        let current = Snapshot {
            data: Arc::new(data),
            id: self.next,
        };
        self.next += 1;
        self.undo
            .push_back(std::mem::replace(&mut self.current, current));
        self.redo.clear();
        while self.undo.len() > 1
            && (self.undo.len() > 12
                || self.undo.iter().map(|s| s.data.len() as u64).sum::<u64>() > 192 * 1024 * 1024)
        {
            self.undo.pop_front();
        }
        self.bookmarks = doc.bookmarks();
        self.doc = doc;
        Ok(())
    }
    pub fn undo(&mut self, redo: bool) -> Result<()> {
        let next = if redo {
            self.redo.last()
        } else {
            self.undo.back()
        };
        if let Some(next) = next {
            let doc = Document::open(next.data.as_ref().clone(), &self.password)?;
            let data = if redo {
                self.redo.pop().unwrap()
            } else {
                self.undo.pop_back().unwrap()
            };
            let previous = std::mem::replace(&mut self.current, data);
            if redo {
                self.undo.push_back(previous)
            } else {
                self.redo.push(previous)
            }
            self.bookmarks = doc.bookmarks();
            self.doc = doc;
        }
        Ok(())
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        save_atomic(path, self.bytes())?;
        self.path = Some(path.into());
        self.saved = Some(self.current.id);
        Ok(())
    }
    pub fn bookmark(&mut self, op: BookmarkOp) -> Result<()> {
        ensure!(
            self.doc.editable(),
            "이 PDF는 편집 권한이 제한되어 있습니다."
        );
        let mut pdf = lopdf::Document::load_mem(self.bytes())?;
        if pdf.is_encrypted() {
            pdf.decrypt(&self.password)?;
        }
        let root = pdf.trailer.get(b"Root")?.as_reference()?;
        let outline = match reference(&pdf, root, b"Outlines") {
            Some(id) => id,
            None => {
                let id = pdf.add_object(dictionary! {"Type"=>"Outlines"});
                pdf.get_dictionary_mut(root)?.set("Outlines", id);
                id
            }
        };
        let mut rows = vec![];
        let mut todo = vec![reference(&pdf, outline, b"First")];
        let mut seen = HashSet::new();
        while let Some(entry) = todo.pop() {
            if let Some(id) = entry {
                if !seen.insert(id) {
                    continue;
                }
                rows.push(id);
                todo.push(reference(&pdf, id, b"Next"));
                todo.push(reference(&pdf, id, b"First"));
            }
        }
        let title = |name: &str| {
            let mut v = vec![0xfe, 0xff];
            v.extend(name.encode_utf16().flat_map(u16::to_be_bytes));
            Object::string_literal(v)
        };
        match op {
            BookmarkOp::Add(name, page, parent) => {
                let pid = match parent {
                    Some(i) => *rows.get(i).context("상위 책갈피")?,
                    None => outline,
                };
                let page_id = *pdf
                    .get_pages()
                    .get(&(page as u32 + 1))
                    .context("페이지 번호")?;
                let mut d = dictionary! {"Title"=>title(&name),"Parent"=>pid,"Dest"=>vec![page_id.into(),Object::Name(b"Fit".to_vec())]};
                let previous = reference(&pdf, pid, b"Last");
                if let Some(last) = previous {
                    d.set("Prev", last)
                }
                let id = pdf.add_object(d);
                if let Some(last) = previous {
                    pdf.get_dictionary_mut(last)?.set("Next", id)
                } else {
                    pdf.get_dictionary_mut(pid)?.set("First", id)
                }
                pdf.get_dictionary_mut(pid)?.set("Last", id);
                update_counts(&mut pdf, pid, 1)?;
            }
            BookmarkOp::Rename(i, name) => {
                let id = *rows.get(i).context("책갈피")?;
                pdf.get_dictionary_mut(id)?.set("Title", title(&name));
            }
            BookmarkOp::Remove(i) => {
                let id = *rows.get(i).context("책갈피")?;
                let parent = reference(&pdf, id, b"Parent").context("상위 책갈피")?;
                let prev = reference(&pdf, id, b"Prev");
                let next = reference(&pdf, id, b"Next");
                set_ref(
                    pdf.get_dictionary_mut(prev.unwrap_or(parent))?,
                    if prev.is_some() { "Next" } else { "First" },
                    next,
                );
                set_ref(
                    pdf.get_dictionary_mut(next.unwrap_or(parent))?,
                    if next.is_some() { "Prev" } else { "Last" },
                    prev,
                );
                let children = pdf
                    .get_dictionary(id)?
                    .get(b"Count")
                    .and_then(Object::as_i64)
                    .unwrap_or(0)
                    .max(0);
                update_counts(&mut pdf, parent, -(1 + children))?;
            }
        }
        if let Some(state) = pdf.encryption_state.clone() {
            pdf.encrypt(&state)?;
        }
        let mut out = vec![];
        pdf.save_to(&mut out)?;
        self.replace(out)
    }
}
pub enum BookmarkOp {
    Add(String, usize, Option<usize>),
    Rename(usize, String),
    Remove(usize),
}
fn reference(pdf: &lopdf::Document, id: ObjectId, key: &[u8]) -> Option<ObjectId> {
    pdf.get_dictionary(id)
        .ok()?
        .get(key)
        .ok()?
        .as_reference()
        .ok()
}
fn set_ref(dict: &mut Dictionary, key: &str, id: Option<ObjectId>) {
    if let Some(i) = id {
        dict.set(key, i)
    } else {
        dict.remove(key.as_bytes());
    }
}
fn update_counts(pdf: &mut lopdf::Document, mut id: ObjectId, delta: i64) -> Result<()> {
    let mut seen = HashSet::new();
    while seen.insert(id) {
        let d = pdf.get_dictionary_mut(id)?;
        let count = d.get(b"Count").and_then(Object::as_i64).unwrap_or(0);
        d.set(
            "Count",
            if count < 0 {
                -(count.saturating_abs() + delta).max(0)
            } else {
                (count + delta).max(0)
            },
        );
        if count < 0 {
            break;
        }
        if let Some(p) = reference(pdf, id, b"Parent") {
            id = p
        } else {
            break;
        }
    }
    Ok(())
}

pub fn pages(input: &str, total: usize) -> Result<Vec<usize>> {
    if input.trim().is_empty() || input.trim() == "전체" {
        return Ok((0..total).collect());
    }
    let mut out = vec![];
    for part in input.split(',') {
        let (a, b) = part.trim().split_once('-').unwrap_or((part, part));
        let a = a.trim().parse::<usize>().context("페이지 범위 예: 1,3-5")?;
        let b = b.trim().parse::<usize>()?;
        ensure!(
            a >= 1 && b >= a && b <= total,
            "페이지 범위는 1부터 {total}까지입니다."
        );
        out.extend(a - 1..b);
    }
    Ok(out)
}
#[derive(Clone, Copy, Default)]
pub struct SearchOptions {
    pub match_case: bool,
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || unicode_normalization::char::is_combining_mark(c)
}

fn fold(c: char, match_case: bool) -> Vec<char> {
    if !match_case && (c == 'ß' || c == 'ẞ') {
        return vec!['s', 's'];
    }
    let text = if match_case {
        c.to_string()
    } else {
        c.to_lowercase().collect()
    };
    text.nfkd()
        .filter(|c| *c != '\u{ad}' && *c != '\u{2}')
        .collect()
}
pub fn hits(glyphs: &[Glyph], query: &str, options: SearchOptions) -> Vec<Vec<Rect>> {
    let needle: Vec<char> = query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .flat_map(|c| fold(c, options.match_case))
        .collect();
    if needle.is_empty() {
        return vec![];
    }
    let code = needle.iter().all(char::is_ascii_alphanumeric)
        && needle.iter().any(char::is_ascii_alphabetic)
        && needle.iter().any(char::is_ascii_digit);
    // Treat a four-digit mixed hexadecimal alarm code as the same token with
    // or without the conventional prefix, without loosening word boundaries.
    let optional_hex_prefix =
        code && needle.len() == 4 && needle.iter().all(char::is_ascii_hexdigit);
    let mut hay = vec![];
    let mut indices = vec![];
    for (i, g) in glyphs.iter().enumerate() {
        if !code
            && g.ch == '-'
            && glyphs
                .get(i + 1)
                .is_some_and(|g| g.ch == '\r' || g.ch == '\n')
        {
            continue;
        }
        for c in fold(g.ch, options.match_case) {
            let c = if c == '\r' || c == '\n' {
                '\n'
            } else if c.is_whitespace() {
                ' '
            } else {
                c
            };
            if c == '\n' && hay.last() == Some(&'\n') {
                continue;
            }
            hay.push(c);
            indices.push(i)
        }
    }
    let mut out = vec![];
    let mut i = 0;
    while i < hay.len() {
        let mut cursor = i;
        if optional_hex_prefix
            && hay.get(i) == Some(&'0')
            && hay.get(i + 1).is_some_and(|c| c.eq_ignore_ascii_case(&'x'))
        {
            cursor += 2;
        }
        let mut matched = true;
        for &c in &needle {
            if c == ' ' {
                let start = cursor;
                while cursor < hay.len() && hay[cursor].is_whitespace() {
                    cursor += 1
                }
                if start == cursor {
                    matched = false;
                    break;
                }
            } else {
                if !code && cursor > i {
                    while hay.get(cursor) == Some(&'\n') {
                        cursor += 1
                    }
                }
                if hay.get(cursor) != Some(&c) {
                    matched = false;
                    break;
                }
                cursor += 1
            }
        }
        if matched && code {
            let before = i.checked_sub(1).and_then(|j| hay.get(j));
            let after = hay.get(cursor);
            matched = !before.is_some_and(|&c| word_char(c))
                && !after.is_some_and(|&c| word_char(c))
                && indices[i..cursor]
                    .iter()
                    .all(|&j| glyphs[j].rect.is_positive());
        }
        if matched {
            let range = indices[i]..=indices[cursor - 1];
            let mut boxes: Vec<Rect> = vec![];
            for g in &glyphs[range] {
                if g.ch.is_whitespace() || !g.rect.is_positive() {
                    continue;
                }
                if let Some(last) = boxes.last_mut() {
                    if (last.center().y - g.rect.center().y).abs()
                        < last.height().max(g.rect.height()) * 0.65
                        && g.rect.left() <= last.right() + 8.
                    {
                        *last = last.union(g.rect);
                        continue;
                    }
                }
                boxes.push(g.rect)
            }
            if !boxes.is_empty() {
                out.push(boxes)
            }
            i = cursor;
        } else {
            i += 1
        }
    }
    out
}
pub fn rows(runs: &[Run]) -> Vec<Vec<&Run>> {
    let mut sorted: Vec<_> = runs.iter().collect();
    sorted.sort_by(|a, b| a.origin.y.total_cmp(&b.origin.y));
    let mut lines: Vec<Vec<&Run>> = vec![];
    for run in sorted {
        if let Some(last) = lines.last_mut() {
            if (last[0].origin.y - run.origin.y).abs() <= run.size.min(last[0].size) * 0.23 {
                last.push(run);
                continue;
            }
        }
        lines.push(vec![run]);
    }
    for line in &mut lines {
        line.sort_by(|a, b| a.rect.left().total_cmp(&b.rect.left()))
    }
    lines
}
pub fn edit_line(layout: &Layout, point: Pos2) -> Vec<Run> {
    for line in rows(&layout.runs) {
        if let Some(index) = line.iter().position(|r| r.rect.expand(2.).contains(point)) {
            let mut start = index;
            let mut end = index + 1;
            while start > 0
                && line[start].rect.left() - line[start - 1].rect.right() < line[start].size * 0.6
            {
                start -= 1
            }
            while end < line.len()
                && line[end].rect.left() - line[end - 1].rect.right() < line[end].size * 0.6
            {
                end += 1
            }
            return line[start..end].iter().map(|r| (*r).clone()).collect();
        }
    }
    vec![]
}
pub fn inferred(
    layout: &Layout,
    height: f32,
    doc: &Document,
    bookmarks: &[Bookmark],
) -> Vec<(Rect, Destination)> {
    let lines = rows(&layout.runs);
    let key = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let title = |line: &Vec<&Run>| {
        line.iter()
            .map(|r| r.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };
    if !lines.iter().any(|r| {
        r[0].rect.top() < height * 0.4
            && matches!(
                key(&title(r)).as_str(),
                "목차" | "차례" | "contents" | "tableofcontents"
            )
    }) {
        return vec![];
    }
    let mut labels: HashMap<String, Vec<usize>> = HashMap::new();
    for p in 0..doc.count() {
        let s = doc.label(p);
        if !s.is_empty() {
            labels.entry(s).or_default().push(p)
        }
    }
    let mut out = vec![];
    for line in lines {
        let s = title(&line);
        let trimmed = s.trim_end();
        let boundary = trimmed.rfind(|c: char| c.is_whitespace() || ".…·".contains(c));
        let Some(last) = boundary else { continue };
        let label =
            trimmed[last..].trim_start_matches(|c: char| c.is_whitespace() || ".…·".contains(c));
        if label.is_empty()
            || !label
                .chars()
                .all(|c| c.is_ascii_digit() || "ivxlcdmIVXLCDM".contains(c))
        {
            continue;
        }
        let name = key(&trimmed[..last]);
        if name.chars().count() < 2 {
            continue;
        }
        let dest = bookmarks
            .iter()
            .find(|b| key(&b.title) == name)
            .and_then(|b| b.dest.clone())
            .or_else(|| {
                let p = if let Some(list) = labels.get(label) {
                    if list.len() != 1 {
                        return None;
                    }
                    list[0]
                } else {
                    label.parse::<usize>().ok()?.checked_sub(1)?
                };
                (p < doc.count()).then_some(Destination {
                    page: p,
                    point: None,
                })
            });
        if let Some(d) = dest {
            out.push((
                line.iter()
                    .fold(Rect::NOTHING, |box_, r| box_.union(r.rect)),
                d,
            ))
        }
    }
    out
}
