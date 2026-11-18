use crate::{
    pdf::{Layout, Run},
    session::rows,
};
use eframe::egui::{Pos2, Rect};
pub struct Table {
    pub rect: Rect,
    pub xs: Vec<f32>,
    pub ys: Vec<f32>,
    pub cells: Vec<Vec<String>>,
}
fn positions(mut values: Vec<f32>) -> Vec<f32> {
    values.sort_by(f32::total_cmp);
    values.dedup_by(|a, b| (*a - *b).abs() < 1.5);
    values
}
fn cell_text(runs: &[&Run]) -> String {
    let mut text = String::new();
    let mut prev: Option<&Run> = None;
    for &r in runs {
        if let Some(p) = prev {
            if (p.origin.y - r.origin.y).abs() > r.size * 0.35 {
                text.push('\n')
            } else if r.rect.left() - p.rect.right() > r.size * 0.15 {
                text.push(' ');
            }
        }
        text.push_str(&r.text);
        prev = Some(r);
    }
    text
}
pub fn ruled(layout: &Layout) -> Vec<Table> {
    // Join touching rules before inferring cells, keeping independent tables on one page separate.
    let mut groups: Vec<(Rect, Vec<(Pos2, Pos2)>)> = vec![];
    for &(a, b) in &layout.rules {
        let delta = (a - b).abs();
        if delta.max_elem() < 4. || delta.min_elem() > 0.7 {
            continue;
        }
        let mut bounds = Rect::from_two_pos(a, b);
        let mut parts = vec![(a, b)];
        let mut i = 0;
        while i < groups.len() {
            if groups[i].0.expand(1.).intersects(bounds) {
                let (g, mut lines) = groups.swap_remove(i);
                bounds = bounds.union(g);
                parts.append(&mut lines);
                i = 0;
            } else {
                i += 1
            }
        }
        groups.push((bounds, parts));
    }
    let mut tables = vec![];
    for (rect, lines) in groups {
        let xs = positions(
            lines
                .iter()
                .filter(|(a, b)| (a.x - b.x).abs() < 0.7)
                .map(|(a, _)| a.x)
                .collect(),
        );
        let ys = positions(
            lines
                .iter()
                .filter(|(a, b)| (a.y - b.y).abs() < 0.7)
                .map(|(a, _)| a.y)
                .collect(),
        );
        if xs.len() < 3 || ys.len() < 3 || xs.len().saturating_mul(ys.len()) > 25000 {
            continue;
        }
        let mut grid = vec![vec![vec![]; xs.len() - 1]; ys.len() - 1];
        for line in rows(&layout.runs) {
            for r in line {
                let c = r.rect.center();
                if !rect.contains(c) {
                    continue;
                }
                let x = xs
                    .partition_point(|v| *v <= c.x)
                    .saturating_sub(1)
                    .min(xs.len() - 2);
                let y = ys
                    .partition_point(|v| *v <= c.y)
                    .saturating_sub(1)
                    .min(ys.len() - 2);
                grid[y][x].push(r);
            }
        }
        let cells: Vec<Vec<_>> = grid
            .into_iter()
            .map(|r| r.into_iter().map(|c| cell_text(&c)).collect())
            .collect();
        if cells.iter().flatten().any(|s| !s.is_empty()) {
            tables.push(Table {
                rect,
                xs,
                ys,
                cells,
            })
        }
    }
    tables.sort_by(|a, b| a.rect.top().total_cmp(&b.rect.top()));
    tables
}
