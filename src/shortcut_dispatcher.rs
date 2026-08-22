use crate::editor::ToolType;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ShortcutPermissions {
    pub(crate) global: bool,
    pub(crate) document: bool,
}

pub(crate) fn shortcut_permissions(
    wants_keyboard_input: bool,
    egui_modal_open: bool,
    popup_open: bool,
    foreground_dialog_open: bool,
) -> ShortcutPermissions {
    let global = !egui_modal_open && !popup_open && !foreground_dialog_open;
    ShortcutPermissions {
        global,
        document: global && !wants_keyboard_input,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShortcutCommand {
    #[cfg(not(target_arch = "wasm32"))]
    ToggleFullscreen,
    CancelCanvasAction,
    SaveProjectAs,
    TogglePlayback,
    Undo,
    Redo,
    NewProject,
    OpenProject,
    SwapColors,
    Deselect,
    SelectAll,
    Copy,
    Cut,
    ClearSelection,
    Paste,
    DecreaseBrushSize,
    IncreaseBrushSize,
    PreviousFrame,
    NextFrame,
    SelectTool(ToolType),
}

/// Converts one egui input snapshot into ordered, model-independent commands.
/// The app coordinator remains responsible for executing commands and deciding
/// whether a state-dependent operation can succeed.
pub(crate) struct ShortcutDispatcher;

impl ShortcutDispatcher {
    pub(crate) fn commands(
        input: &egui::InputState,
        permissions: ShortcutPermissions,
    ) -> Vec<ShortcutCommand> {
        let mut commands = Vec::new();

        #[cfg(not(target_arch = "wasm32"))]
        if input.key_pressed(egui::Key::F11) {
            commands.push(ShortcutCommand::ToggleFullscreen);
        }
        if input.key_pressed(egui::Key::Escape) {
            commands.push(ShortcutCommand::CancelCanvasAction);
        }
        if permissions.global && input.modifiers.ctrl && input.key_pressed(egui::Key::S) {
            commands.push(ShortcutCommand::SaveProjectAs);
        }
        if !permissions.document {
            return commands;
        }

        if !input.modifiers.ctrl && input.key_pressed(egui::Key::Space) {
            commands.push(ShortcutCommand::TogglePlayback);
        }
        if input.modifiers.ctrl && !input.modifiers.shift && input.key_pressed(egui::Key::Z) {
            commands.push(ShortcutCommand::Undo);
        }
        if input.modifiers.ctrl
            && (input.key_pressed(egui::Key::Y)
                || (input.modifiers.shift && input.key_pressed(egui::Key::Z)))
        {
            commands.push(ShortcutCommand::Redo);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::N) {
            commands.push(ShortcutCommand::NewProject);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::O) {
            commands.push(ShortcutCommand::OpenProject);
        }
        if !input.modifiers.ctrl && input.key_pressed(egui::Key::X) {
            commands.push(ShortcutCommand::SwapColors);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::D) {
            commands.push(ShortcutCommand::Deselect);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::A) {
            commands.push(ShortcutCommand::SelectAll);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::C) {
            commands.push(ShortcutCommand::Copy);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::X) {
            commands.push(ShortcutCommand::Cut);
        }
        if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
            commands.push(ShortcutCommand::ClearSelection);
        }
        if input.modifiers.ctrl && input.key_pressed(egui::Key::V) {
            commands.push(ShortcutCommand::Paste);
        }
        if input.key_pressed(egui::Key::OpenBracket) {
            commands.push(ShortcutCommand::DecreaseBrushSize);
        }
        if input.key_pressed(egui::Key::CloseBracket) {
            commands.push(ShortcutCommand::IncreaseBrushSize);
        }
        if input.key_pressed(egui::Key::Comma) {
            commands.push(ShortcutCommand::PreviousFrame);
        }
        if input.key_pressed(egui::Key::Period) {
            commands.push(ShortcutCommand::NextFrame);
        }

        for (key, tool) in [
            (egui::Key::H, ToolType::Hand),
            (egui::Key::M, ToolType::Marquee),
            (egui::Key::V, ToolType::Move),
            (egui::Key::B, ToolType::Pencil),
            (egui::Key::E, ToolType::Eraser),
            (egui::Key::L, ToolType::Line),
            (egui::Key::R, ToolType::Rectangle),
            (egui::Key::O, ToolType::Ellipse),
            (egui::Key::G, ToolType::Fill),
            (egui::Key::I, ToolType::Eyedropper),
        ] {
            if !input.modifiers.ctrl && input.key_pressed(key) {
                commands.push(ShortcutCommand::SelectTool(tool));
            }
        }
        if !input.modifiers.ctrl && !input.modifiers.shift && input.key_pressed(egui::Key::Z) {
            commands.push(ShortcutCommand::SelectTool(ToolType::Zoom));
        }

        commands
    }
}
