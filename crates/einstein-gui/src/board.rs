use crate::navigation::{Navigation, Zone};
use crate::theme::Classic;

#[derive(Clone, Copy)]
pub(crate) struct Skin<'a> {
    pub(crate) zoom: f32,
    pub(crate) classic: Option<&'a Classic>,
}

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Role, Shape, Stroke, Ui, Vec2, WidgetInfo,
};
use einstein_core::session::{Command, Hint, HintView, Snapshot};
use einstein_core::{Card, Category, Column, Rule, RulePlacement, Value};

const SURFACE: Color32 = Color32::from_rgb(20, 32, 53);
const LINE: Color32 = Color32::from_rgb(57, 78, 103);
const SUBDUED: Color32 = Color32::from_rgb(160, 179, 203);
const CATEGORIES: [&str; 6] = [
    "Chiffres", "Lettres", "Romains", "Dés", "Formes", "Symboles",
];
const ACCENTS: [Color32; 6] = [
    Color32::from_rgb(111, 211, 255),
    Color32::from_rgb(139, 233, 178),
    Color32::from_rgb(235, 199, 117),
    Color32::from_rgb(255, 147, 166),
    Color32::from_rgb(197, 161, 255),
    Color32::from_rgb(122, 232, 221),
];
const ROMAN: [&str; 6] = ["I", "II", "III", "IV", "V", "VI"];
const SIGNS: [&str; 6] = ["+", "−", "÷", "×", "=", "√"];
const FORMES: [&str; 6] = [
    "triangle pointe en haut",
    "triangle pointe en bas",
    "carré",
    "losange",
    "pentagone pointe en haut",
    "pentagone pointe en bas",
];
const MATH_NAMES: [&str; 6] = [
    "plus",
    "moins",
    "division",
    "multiplication",
    "égalité",
    "racine carrée",
];

fn label(card: Card) -> String {
    let index = usize::from(card.value.number() - 1);
    match card.category {
        Category::ArabicDigits => format!("chiffre {}", card.value.number()),
        Category::Letters => format!("lettre {}", char::from(b'A' + index as u8)),
        Category::RomanNumerals => format!("romain {}", ROMAN[index]),
        Category::Dice => format!("dé {}", card.value.number()),
        Category::Shapes => FORMES[index].to_owned(),
        Category::MathSymbols => MATH_NAMES[index].to_owned(),
    }
}

fn glyph(ui: &Ui, rect: Rect, card: Card, size: f32, skin: Skin<'_>) {
    if let Some(classic) = skin.classic {
        let side = (size * 1.42).min(rect.width()).min(rect.height());
        classic.card(
            ui.painter(),
            Rect::from_center_size(rect.center(), Vec2::splat(side)),
            card,
        );
        return;
    }
    let i = usize::from(card.value.number() - 1);
    let color = ACCENTS[card.category.index()];
    let painter = ui.painter();
    let c = rect.center();
    if card.category == Category::Dice {
        let side = size * 0.82;
        let r = Rect::from_center_size(c, Vec2::splat(side));
        painter.rect_stroke(r, 2, Stroke::new(1.25, color), egui::StrokeKind::Inside);
        let dots: &[(i8, i8)] = match i {
            0 => &[(0, 0)],
            1 => &[(-1, -1), (1, 1)],
            2 => &[(-1, -1), (0, 0), (1, 1)],
            3 => &[(-1, -1), (-1, 1), (1, -1), (1, 1)],
            4 => &[(-1, -1), (-1, 1), (0, 0), (1, -1), (1, 1)],
            _ => &[(-1, -1), (-1, 0), (-1, 1), (1, -1), (1, 0), (1, 1)],
        };
        for &(x, y) in dots {
            painter.circle_filled(
                c + Vec2::new(f32::from(x), f32::from(y)) * side * 0.26,
                (side * 0.08).max(1.1),
                color,
            );
        }
    } else if card.category == Category::Shapes {
        let radius = size * 0.45;
        if i == 2 {
            painter.rect_filled(
                Rect::from_center_size(c, Vec2::splat(radius * 1.6)),
                1,
                color,
            );
        } else {
            let count = match i {
                0 | 1 => 3,
                3 => 4,
                _ => 5,
            };
            let rotation = if i == 1 || i == 5 {
                std::f32::consts::FRAC_PI_2
            } else {
                -std::f32::consts::FRAC_PI_2
            };
            let points = (0..count)
                .map(|j| {
                    let a = j as f32 * std::f32::consts::TAU / count as f32 + rotation;
                    Pos2::new(c.x + a.cos() * radius, c.y + a.sin() * radius)
                })
                .collect();
            painter.add(Shape::convex_polygon(points, color, Stroke::NONE));
        }
    } else {
        let text = match card.category {
            Category::ArabicDigits => card.value.number().to_string(),
            Category::Letters => char::from(b'A' + i as u8).to_string(),
            Category::RomanNumerals => ROMAN[i].to_owned(),
            Category::MathSymbols => SIGNS[i].to_owned(),
            _ => unreachable!("painted separately"),
        };
        painter.text(
            c,
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(size),
            color,
        );
    }
}

fn accessible_button(ui: &mut Ui, size: Vec2, spoken: &str, enabled: bool) -> egui::Response {
    let response = ui
        .add_enabled_ui(enabled, |ui| ui.add_sized(size, egui::Button::new("")))
        .inner;
    response.widget_info(|| WidgetInfo::labeled(Role::Button, enabled, spoken));
    response
}

pub(crate) fn grid(
    ui: &mut Ui,
    state: &Snapshot,
    skin: Skin<'_>,
    active: bool,
    command: &mut Option<Command>,
    nav: &mut Navigation,
) {
    let slot = Vec2::new(25.0, 25.0) * skin.zoom;
    let width = slot.x * 3.0 + 6.0;
    let row_label = 70.0;
    ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [row_label, 22.0],
            egui::Label::new(RichText::new("CATÉGORIE").color(SUBDUED).small()),
        );
        for col in Column::ALL {
            ui.add_sized(
                [width + 8.0, 22.0],
                egui::Label::new(
                    RichText::new(format!("Col. {}", col.number()))
                        .color(SUBDUED)
                        .strong(),
                ),
            );
        }
    });
    for cat in Category::ALL {
        ui.horizontal(|ui| {
            ui.add_sized(
                [row_label, slot.y * 2.0 + 12.0],
                egui::Label::new(
                    RichText::new(CATEGORIES[cat.index()])
                        .color(ACCENTS[cat.index()])
                        .strong(),
                ),
            );
            for col in Column::ALL {
                ui.push_id(("cell", cat.index(), col.number()), |ui| {
                    egui::Frame::new()
                        .fill(SURFACE)
                        .stroke(Stroke::new(1.0, LINE))
                        .inner_margin(3)
                        .corner_radius(6)
                        .show(ui, |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                let mask = state.candidates.mask(cat, col);
                                if mask.count_ones() == 1 {
                                    let n = mask.trailing_zeros() as usize;
                                    let card = Card::new(cat, Value::ALL[n]);
                                    let response = ui.add_sized(
                                        [width, slot.y * 2.0 + 3.0],
                                        egui::Label::new(""),
                                    );
                                    response.widget_info(|| {
                                        WidgetInfo::labeled(
                                            Role::Label,
                                            true,
                                            format!(
                                                "{}, colonne {}, confirmé : {}",
                                                CATEGORIES[cat.index()],
                                                col.number(),
                                                label(card)
                                            ),
                                        )
                                    });
                                    glyph(ui, response.rect, card, slot.x * 1.08, skin);
                                } else {
                                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                                    for offset in [0, 3] {
                                        ui.horizontal(|ui| {
                                            for value in &Value::ALL[offset..offset + 3] {
                                                let card = Card::new(cat, *value);
                                                if state.candidates.allows(card, col) {
                                                    let name = format!(
                                                        "{}, colonne {}, candidat {}",
                                                        CATEGORIES[cat.index()],
                                                        col.number(),
                                                        label(card)
                                                    );
                                                    let response =
                                                        accessible_button(ui, slot, &name, active)
                                                            .on_hover_text(&name);
                                                    if active {
                                                        nav.item(Zone::Grid, &response);
                                                    }
                                                    glyph(
                                                        ui,
                                                        response.rect,
                                                        card,
                                                        slot.x * 0.59,
                                                        skin,
                                                    );
                                                    let exclusion = response.secondary_clicked()
                                                        || (response.has_focus()
                                                            && ui.input(|i| {
                                                                i.key_pressed(egui::Key::Delete)
                                                            }));
                                                    if active && exclusion {
                                                        *command = Some(Command::Exclude {
                                                            card,
                                                            column: col,
                                                        });
                                                    } else if active && response.clicked() {
                                                        *command = Some(Command::Select {
                                                            card,
                                                            column: col,
                                                        });
                                                    }
                                                } else {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        slot,
                                                        egui::Sense::hover(),
                                                    );
                                                    ui.painter().rect_filled(
                                                        rect,
                                                        3,
                                                        Color32::from_rgb(16, 25, 43),
                                                    );
                                                }
                                            }
                                        });
                                    }
                                }
                            })
                        });
                });
            }
        });
    }
}

fn hint_text(rule: Rule) -> (&'static str, String, Vec<Card>) {
    match rule {
        Rule::SameColumn(a, b) => (
            "Même colonne",
            format!("{} et {} sont dans la même colonne", label(a), label(b)),
            vec![a, b],
        ),
        Rule::Near(a, b) => (
            "Colonnes voisines",
            format!("{} est voisin de {}", label(a), label(b)),
            vec![a, b],
        ),
        Rule::LeftOf { left, right } => (
            "À gauche",
            format!(
                "{} est strictement à gauche de {}",
                label(left),
                label(right)
            ),
            vec![left, right],
        ),
        Rule::Between {
            middle,
            ends: [a, b],
        } => (
            "Trois colonnes",
            format!(
                "{} est entre {} et {}, trois colonnes consécutives",
                label(middle),
                label(a),
                label(b)
            ),
            vec![a, middle, b],
        ),
        Rule::Reveal { .. } => unreachable!("not a visible hint"),
    }
}

#[allow(clippy::too_many_arguments)]
fn clue(
    ui: &mut Ui,
    hint: Hint,
    view: HintView,
    width: f32,
    skin: Skin<'_>,
    active: bool,
    command: &mut Option<Command>,
    nav: &mut Navigation,
) {
    let zoom = skin.zoom;
    let height = 42.0 * zoom;
    let size = egui::vec2(width, height);
    if !hint.visible(view) {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_stroke(rect, 5, Stroke::new(1.0, LINE), egui::StrokeKind::Inside);
        return;
    }
    let (tag, spoken, cards) = hint_text(hint.rule);
    let button = accessible_button(ui, size, &spoken, active).on_hover_text(&spoken);
    if active {
        let zone = if hint.rule.placement() == RulePlacement::Vertical {
            Zone::Columns
        } else {
            Zone::Relations
        };
        nav.item(zone, &button);
    }
    let painter = ui.painter();
    let rect = button.rect;
    painter.text(
        rect.left_top() + egui::vec2(7.0, 3.0),
        Align2::LEFT_TOP,
        tag,
        FontId::proportional(12.0),
        SUBDUED,
    );
    if let Some(classic) = skin.classic {
        let marker_height = match hint.rule {
            Rule::Near(..) | Rule::LeftOf { .. } => 19.0 * zoom,
            _ => 8.0 * zoom,
        };
        classic.mark(
            painter,
            Pos2::new(rect.right() - 27.0 * zoom, rect.center().y),
            marker_height,
            hint.rule,
        );
    }
    for (i, card) in cards.into_iter().enumerate() {
        let step = (width - 14.0).min(92.0) / 3.0;
        let point = Pos2::new(
            rect.left() + 14.0 + i as f32 * step,
            rect.bottom() - height * 0.32,
        );
        glyph(
            ui,
            Rect::from_center_size(point, Vec2::splat(19.0 * zoom)),
            card,
            16.0 * zoom,
            skin,
        );
    }
    if active && (button.secondary_clicked() || button.clicked()) {
        *command = Some(Command::ToggleHint { id: hint.id });
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn clues(
    ui: &mut Ui,
    state: &Snapshot,
    placement: RulePlacement,
    tile_width: f32,
    skin: Skin<'_>,
    active: bool,
    command: &mut Option<Command>,
    nav: &mut Navigation,
) {
    let hints = state
        .hints
        .iter()
        .copied()
        .filter(|hint| hint.rule.placement() == placement)
        .collect::<Vec<_>>();
    if hints.is_empty() {
        ui.label(RichText::new("Aucun indice dans cette famille.").color(SUBDUED));
    }
    if placement == RulePlacement::Vertical {
        ui.horizontal(|ui| {
            for hint in hints {
                ui.push_id(hint.id.0, |ui| {
                    clue(
                        ui,
                        hint,
                        state.hint_view,
                        tile_width,
                        skin,
                        active,
                        command,
                        nav,
                    )
                });
            }
        });
    } else {
        for hint in hints {
            ui.push_id(hint.id.0, |ui| {
                clue(
                    ui,
                    hint,
                    state.hint_view,
                    tile_width,
                    skin,
                    active,
                    command,
                    nav,
                )
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use einstein_core::session::Session;

    #[test]
    fn accesskit_update_has_named_candidate_and_clue_buttons() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut session = Session::new();
        session.apply(Command::New { seed: 42 }).unwrap();
        let state = session.snapshot().unwrap();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut command = None;
            let mut nav = Navigation::default();
            grid(
                ui,
                &state,
                Skin {
                    zoom: 1.0,
                    classic: None,
                },
                true,
                &mut command,
                &mut nav,
            );
            clues(
                ui,
                &state,
                RulePlacement::Horizontal,
                150.0,
                Skin {
                    zoom: 1.0,
                    classic: None,
                },
                true,
                &mut command,
                &mut nav,
            );
        });
        output.textures_delta.clear(); // This test inspects semantics, not texture uploads.
        let tree = output
            .platform_output
            .accesskit_update
            .expect("egui AccessKit tree");
        let is_button = |name: &str| {
            tree.nodes.iter().any(|(_, node)| {
                node.label() == Some(name) && node.role() == egui::accesskit::Role::Button
            })
        };
        assert!(is_button("Chiffres, colonne 1, candidat chiffre 1"));
        let (_, clue, _) = hint_text(
            state
                .hints
                .iter()
                .find(|h| h.rule.placement() == RulePlacement::Horizontal)
                .unwrap()
                .rule,
        );
        assert!(is_button(&clue));
    }
}
