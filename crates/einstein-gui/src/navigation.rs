//! Keyboard navigation between the three independently scrolling play areas.
use eframe::egui::{self, Align, Id, Response, Ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Zone {
    Grid,
    Columns,
    Relations,
}

impl Zone {
    const ALL: [Self; 3] = [Self::Grid, Self::Columns, Self::Relations];
}

// Egui's consume_key also matches extra Shift and Alt; the two F6 shortcuts
// must be disjoint, even when multiple key events arrive in one frame.
fn consume_exact(
    events: &mut Vec<egui::Event>,
    modifiers: egui::Modifiers,
    key: egui::Key,
) -> bool {
    let before = events.len();
    events.retain(|event| {
        !matches!(event, egui::Event::Key { key: event_key, modifiers: event_modifiers, pressed: true, .. }
            if *event_key == key && event_modifiers.matches_exact(modifiers))
    });
    events.len() != before
}

#[derive(Default)]
pub(crate) struct Navigation {
    // Keep only the previous rendered frame's eligible control IDs.
    previous: [Vec<Id>; 3],
    rendered: [Vec<Id>; 3],
    reveal: Option<Id>,
}

impl Navigation {
    pub(crate) fn begin(&mut self, ui: &Ui) {
        for list in &mut self.rendered {
            list.clear();
        }
        self.reveal = None;
        let focused = ui.memory(|m| m.focused());
        let zone = focused.and_then(|id| self.previous.iter().position(|list| list.contains(&id)));
        let keys = ui.input_mut(|i| {
            let forward = consume_exact(&mut i.events, egui::Modifiers::NONE, egui::Key::F6);
            let backward = consume_exact(&mut i.events, egui::Modifiers::SHIFT, egui::Key::F6);
            let home = zone.is_some()
                && consume_exact(&mut i.events, egui::Modifiers::CTRL, egui::Key::Home);
            let end = zone.is_some()
                && consume_exact(&mut i.events, egui::Modifiers::CTRL, egui::Key::End);
            (forward, backward, home, end)
        });
        let destination = match keys {
            (true, _, _, _) => self.cycle(zone, false),
            (_, true, _, _) => self.cycle(zone, true),
            (_, _, true, _) => zone.and_then(|z| self.previous[z].first().copied()),
            (_, _, _, true) => zone.and_then(|z| self.previous[z].last().copied()),
            _ => None,
        };
        if let Some(id) = destination {
            ui.memory_mut(|m| m.request_focus(id));
            self.reveal = Some(id);
        }
    }

    fn cycle(&self, zone: Option<usize>, reverse: bool) -> Option<Id> {
        let start = zone.unwrap_or(if reverse { 0 } else { 2 });
        (1..=3).find_map(|step| {
            let i = if reverse {
                (start + 3 - step) % 3
            } else {
                (start + step) % 3
            };
            self.previous[i].first().copied()
        })
    }

    pub(crate) fn item(&mut self, zone: Zone, response: &Response) {
        let index = Zone::ALL
            .iter()
            .position(|&z| z == zone)
            .expect("valid zone");
        self.rendered[index].push(response.id);
        if response.gained_focus() || self.reveal == Some(response.id) {
            response.scroll_to_me(Some(Align::Center));
            response.ctx.request_repaint();
        }
        if response.has_focus() {
            response
                .ctx
                .debug_painter()
                .with_clip_rect(response.interact_rect)
                .rect_stroke(
                    response.rect,
                    3,
                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 215, 114)),
                    egui::StrokeKind::Inside,
                );
        }
    }

    pub(crate) fn end(&mut self) {
        std::mem::swap(&mut self.previous, &mut self.rendered);
    }

    pub(crate) fn clear(&mut self) {
        self.previous.iter_mut().for_each(Vec::clear);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shift_f6_is_not_forward_f6() {
        let mut events = vec![egui::Event::Key {
            key: egui::Key::F6,
            physical_key: Some(egui::Key::F6),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::SHIFT,
        }];
        assert!(!consume_exact(
            &mut events,
            egui::Modifiers::NONE,
            egui::Key::F6
        ));
        assert!(consume_exact(
            &mut events,
            egui::Modifiers::SHIFT,
            egui::Key::F6
        ));
        assert!(events.is_empty());
    }

    #[test]
    fn zone_cycle_wraps_and_skips_an_empty_pane() {
        let grid = Id::unique("navigation-test-grid");
        let rel = Id::unique("navigation-test-relations");
        let nav = Navigation {
            previous: [vec![grid], vec![], vec![rel]],
            ..Default::default()
        };
        assert_eq!(nav.cycle(None, false), Some(grid));
        assert_eq!(nav.cycle(None, true), Some(rel));
        assert_eq!(nav.cycle(Some(0), false), Some(rel));
        assert_eq!(nav.cycle(Some(2), true), Some(grid));
    }
}
