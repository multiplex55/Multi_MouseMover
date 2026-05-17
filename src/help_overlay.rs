use crate::action::Action;
use crate::key_chord::KeyChord;
use std::sync::RwLock;

lazy_static::lazy_static! {
    static ref HELP_OVERLAY: RwLock<Option<HelpOverlayView>> = RwLock::new(None);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpBinding {
    pub key: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpOverlayView {
    pub bindings: Vec<HelpBinding>,
}

pub fn help_view_from_bindings<I>(bindings: I) -> HelpOverlayView
where
    I: IntoIterator<Item = (KeyChord, Action)>,
{
    let mut bindings: Vec<HelpBinding> = bindings
        .into_iter()
        .map(|(chord, action)| HelpBinding {
            key: format_key_chord(chord),
            action: format_action(action),
        })
        .collect();
    bindings.sort_by(|left, right| {
        left.action
            .cmp(&right.action)
            .then(left.key.cmp(&right.key))
    });
    HelpOverlayView { bindings }
}

pub fn show_help_overlay(view: HelpOverlayView) {
    println!("[help] showing {} configured bindings", view.bindings.len());
    *HELP_OVERLAY.write().unwrap_or_else(|e| e.into_inner()) = Some(view);
}

pub fn hide_help_overlay() {
    *HELP_OVERLAY.write().unwrap_or_else(|e| e.into_inner()) = None;
}

fn format_action(action: Action) -> String {
    match action {
        Action::JumpModeProfile(profile) => format!("jump_mode_profile:{profile}"),
        other => format!("{other:?}")
            .chars()
            .enumerate()
            .flat_map(|(index, ch)| {
                if index > 0 && ch.is_ascii_uppercase() {
                    vec!['_', ch.to_ascii_lowercase()]
                } else {
                    vec![ch.to_ascii_lowercase()]
                }
            })
            .collect(),
    }
}

fn format_key_chord(chord: KeyChord) -> String {
    let mut parts = Vec::new();
    if chord.ctrl {
        parts.push("Ctrl".to_string());
    }
    if chord.right_alt {
        parts.push("RightAlt".to_string());
    } else if chord.alt {
        parts.push("Alt".to_string());
    }
    if chord.shift {
        parts.push("Shift".to_string());
    }
    if chord.win {
        parts.push("Win".to_string());
    }
    parts.push(format!("{:?}", chord.key));
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::VirtualKey;

    #[test]
    fn help_contents_are_generated_from_keybindings() {
        let view = help_view_from_bindings([
            (KeyChord::from_key(VirtualKey::F), Action::JumpMode),
            (KeyChord::from_key(VirtualKey::H), Action::ShowHelp),
        ]);

        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "F" && binding.action == "jump_mode" }));
        assert!(view
            .bindings
            .iter()
            .any(|binding| { binding.key == "H" && binding.action == "show_help" }));
    }
}
