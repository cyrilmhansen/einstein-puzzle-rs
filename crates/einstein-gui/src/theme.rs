//! Rendering resources only; no effect on a puzzle or its accessibility tree.
use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, Vec2};
use einstein_core::{Card, Rule};

macro_rules! bmp {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/classic/", $name, ".bmp"))
    };
}

const CARDS: [&[u8]; 36] = [
    bmp!("A1"),
    bmp!("A2"),
    bmp!("A3"),
    bmp!("A4"),
    bmp!("A5"),
    bmp!("A6"),
    bmp!("B1"),
    bmp!("B2"),
    bmp!("B3"),
    bmp!("B4"),
    bmp!("B5"),
    bmp!("B6"),
    bmp!("C1"),
    bmp!("C2"),
    bmp!("C3"),
    bmp!("C4"),
    bmp!("C5"),
    bmp!("C6"),
    bmp!("D1"),
    bmp!("D2"),
    bmp!("D3"),
    bmp!("D4"),
    bmp!("D5"),
    bmp!("D6"),
    bmp!("E1"),
    bmp!("E2"),
    bmp!("E3"),
    bmp!("E4"),
    bmp!("E5"),
    bmp!("E6"),
    bmp!("F1"),
    bmp!("F2"),
    bmp!("F3"),
    bmp!("F4"),
    bmp!("F5"),
    bmp!("F6"),
];
const RAIN: &[u8] = bmp!("rain");
const MARKS: [&[u8]; 3] = [bmp!("hint-near"), bmp!("hint-side"), bmp!("betwarr")];
const UNIT: Rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

pub(crate) struct Classic {
    cards: Vec<TextureHandle>,
    marks: Vec<TextureHandle>,
    rain: TextureHandle,
}

impl Classic {
    pub(crate) fn load(ctx: &egui::Context) -> Self {
        let texture = |name, bytes| {
            ctx.load_texture(
                name,
                decode_bmp(bytes).expect("audited built-in BMP"),
                egui::TextureOptions::NEAREST,
            )
        };
        Self {
            cards: CARDS
                .iter()
                .enumerate()
                .map(|(i, &b)| texture(format!("classic-card-{i}"), b))
                .collect(),
            marks: MARKS
                .iter()
                .enumerate()
                .map(|(i, &b)| texture(format!("classic-relation-{i}"), b))
                .collect(),
            rain: ctx.load_texture(
                "classic-rain",
                decode_bmp(RAIN).expect("audited built-in BMP"),
                egui::TextureOptions::NEAREST_REPEAT,
            ),
        }
    }

    pub(crate) fn wallpaper(&self, ui: &egui::Ui) {
        let rect = ui.max_rect();
        let tex = self.rain.size_vec2();
        let uv = Rect::from_min_size(Pos2::ZERO, rect.size() / tex);
        ui.painter()
            .image(self.rain.id(), rect, uv, Color32::from_rgb(111, 119, 127));
    }

    pub(crate) fn card(&self, painter: &egui::Painter, rect: Rect, card: Card) {
        let index = card.category.index() * 6 + usize::from(card.value.number() - 1);
        painter.image(self.cards[index].id(), rect, UNIT, Color32::WHITE);
    }

    pub(crate) fn mark(&self, painter: &egui::Painter, center: Pos2, height: f32, rule: Rule) {
        let i = match rule {
            Rule::Near(..) => 0,
            Rule::LeftOf { .. } => 1,
            Rule::Between { .. } => 2,
            _ => return,
        };
        let texture = &self.marks[i];
        let size = texture.size_vec2();
        let width = height * size.x / size.y;
        painter.image(
            texture.id(),
            Rect::from_center_size(center, Vec2::new(width, height)),
            UNIT,
            Color32::WHITE,
        );
    }
}

/// Restricted to the built-in Windows DIB 40-byte, RGB24, bottom-up files.
fn decode_bmp(bytes: &[u8]) -> Option<egui::ColorImage> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(bytes.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(bytes.get(i..i + 4)?.try_into().ok()?));
    if bytes.get(..2)? != b"BM"
        || u32_at(2)? as usize != bytes.len()
        || u32_at(10)? != 54
        || u32_at(14)? != 40
        || u16_at(26)? != 1
        || u16_at(28)? != 24
        || u32_at(30)? != 0
    {
        return None;
    }
    let (width, height) = (u32_at(18)? as usize, u32_at(22)? as usize);
    if !(1..=256).contains(&width) || !(1..=256).contains(&height) {
        return None;
    }
    let stride = (width * 3).next_multiple_of(4);
    let data = bytes.get(54..)?;
    if data.len() != stride * height {
        return None;
    }
    let mut rgb = Vec::with_capacity(width * height * 3);
    for row in data.chunks_exact(stride).rev() {
        for bgr in row[..width * 3].as_chunks::<3>().0 {
            rgb.extend([bgr[2], bgr[1], bgr[0]]);
        }
    }
    Some(egui::ColorImage::from_rgb([width, height], &rgb))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_allowlisted_graphics_decode() {
        for bytes in CARDS.into_iter().chain(MARKS).chain([RAIN]) {
            assert!(decode_bmp(bytes).is_some());
        }
        assert_eq!(decode_bmp(CARDS[32]).unwrap().size, [48, 48]);
        assert_eq!(decode_bmp(RAIN).unwrap().size, [108, 108]);
        assert!(decode_bmp(&RAIN[..RAIN.len() - 1]).is_none());
        assert!(decode_bmp(b"BM").is_none());
    }
}
