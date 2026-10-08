//! 세이브 스테이트 슬롯: F1–F10 저장, Shift+F1–F10 불러오기 (스펙 §6.2), 툴바 "스테이트" 메뉴.

use aragorn_app::session::{STATE_SLOTS, Session, THUMBNAIL_HEIGHT, THUMBNAIL_WIDTH};
use eframe::egui::{self, Event, Key};

const SLOT_KEYS: [Key; STATE_SLOTS as usize] = [
    Key::F1,
    Key::F2,
    Key::F3,
    Key::F4,
    Key::F5,
    Key::F6,
    Key::F7,
    Key::F8,
    Key::F9,
    Key::F10,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotCommand {
    Save(u8),
    Load(u8),
}

/// 이번 화면 갱신의 키 입력에서 슬롯 단축키를 찾는다. 키 반복은 세지 않는다.
pub fn hotkey(events: &[Event]) -> Option<SlotCommand> {
    events.iter().find_map(|e| match e {
        Event::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } => {
            let slot = SLOT_KEYS.iter().position(|k| k == key)? as u8;
            Some(if modifiers.shift {
                SlotCommand::Load(slot)
            } else {
                SlotCommand::Save(slot)
            })
        }
        _ => None,
    })
}

/// 저장한 지 얼마나 지났는지.
pub fn ago_text(now: u64, saved_at: u64) -> String {
    let secs = now.saturating_sub(saved_at);
    match secs {
        0..60 => "방금".to_string(),
        60..3600 => format!("{}분 전", secs / 60),
        3600..86_400 => format!("{}시간 전", secs / 3600),
        _ => format!("{}일 전", secs / 86_400),
    }
}

struct SlotEntry {
    saved_at: u64,
    texture: Option<egui::TextureHandle>,
}

/// 메뉴에 보일 슬롯 목록. 메뉴를 열 때 비어 있으면 저장소에서 다시 읽는다.
#[derive(Default)]
pub struct SlotsView {
    slots: Option<Vec<Option<SlotEntry>>>,
}

impl SlotsView {
    /// ROM을 바꾸거나 슬롯에 저장하면 다음에 메뉴를 열 때 다시 읽는다.
    pub fn invalidate(&mut self) {
        self.slots = None;
    }

    fn load(ctx: &egui::Context, session: &Session) -> Vec<Option<SlotEntry>> {
        (0..STATE_SLOTS)
            .map(|slot| {
                let info = session.slot_info(slot)?;
                let texture = info.thumbnail.map(|thumb| {
                    let image = egui::ColorImage::from_rgba_unmultiplied(
                        [THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT],
                        &super::screen::to_rgba(&thumb),
                    );
                    ctx.load_texture(format!("slot{slot}"), image, egui::TextureOptions::LINEAR)
                });
                Some(SlotEntry {
                    saved_at: info.saved_at,
                    texture,
                })
            })
            .collect()
    }

    /// 메뉴 안을 그린다. 누른 명령을 돌려준다.
    pub fn show(&mut self, ui: &mut egui::Ui, session: &Session, now: u64) -> Option<SlotCommand> {
        let slots = self
            .slots
            .get_or_insert_with(|| Self::load(ui.ctx(), session));
        let mut command = None;
        ui.label("F1–F10: 저장, Shift+F1–F10: 불러오기");
        ui.separator();
        egui::Grid::new("state_slots").striped(true).show(ui, |ui| {
            for (slot, entry) in slots.iter().enumerate() {
                let slot = slot as u8;
                let size = egui::vec2(THUMBNAIL_WIDTH as f32, THUMBNAIL_HEIGHT as f32);
                match entry.as_ref().and_then(|e| e.texture.as_ref()) {
                    Some(texture) => {
                        ui.add(egui::Image::new((texture.id(), size)));
                    }
                    None => {
                        ui.allocate_space(size);
                    }
                }
                ui.vertical(|ui| {
                    ui.strong(format!("슬롯 {} (F{})", slot + 1, slot + 1));
                    match entry {
                        Some(e) => ui.label(ago_text(now, e.saved_at)),
                        None => ui.weak("비어 있음"),
                    };
                });
                if ui.button("저장").clicked() {
                    command = Some(SlotCommand::Save(slot));
                }
                if ui
                    .add_enabled(entry.is_some(), egui::Button::new("불러오기"))
                    .clicked()
                {
                    command = Some(SlotCommand::Load(slot));
                }
                ui.end_row();
            }
        });
        if command.is_some() {
            ui.close();
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, shift: bool, repeat: bool) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers: egui::Modifiers {
                shift,
                ..Default::default()
            },
        }
    }

    #[test]
    fn function_keys_save_and_shift_loads() {
        assert_eq!(
            hotkey(&[key(Key::F1, false, false)]),
            Some(SlotCommand::Save(0))
        );
        assert_eq!(
            hotkey(&[key(Key::F10, false, false)]),
            Some(SlotCommand::Save(9))
        );
        assert_eq!(
            hotkey(&[key(Key::F3, true, false)]),
            Some(SlotCommand::Load(2))
        );
    }

    #[test]
    fn other_keys_and_repeats_do_nothing() {
        assert_eq!(hotkey(&[]), None);
        assert_eq!(hotkey(&[key(Key::F11, false, false)]), None);
        assert_eq!(hotkey(&[key(Key::A, false, false)]), None);
        assert_eq!(hotkey(&[key(Key::F2, false, true)]), None);
    }

    #[test]
    fn ago_text_uses_largest_unit() {
        assert_eq!(ago_text(100, 100), "방금");
        assert_eq!(ago_text(100, 200), "방금", "시계가 거꾸로 가도 방금");
        assert_eq!(ago_text(1000, 1000 - 5 * 60), "5분 전");
        assert_eq!(ago_text(100_000, 100_000 - 3 * 3600 - 1), "3시간 전");
        assert_eq!(ago_text(1_000_000, 1_000_000 - 2 * 86_400), "2일 전");
    }
}
