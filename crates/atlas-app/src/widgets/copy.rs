//! A labelled copy command: puts projected text on the clipboard and says so.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Sizable as _, WindowExt as _, button::Button};
use gpui_kit::*;

/// `label` names what is copied (`Copy explanation`, `Copy statement`); the
/// text is the current viewer's projection, prepared by the caller.
pub fn copy_button(id: impl Into<ElementId>, label: &'static str, text: impl Into<String>) -> impl IntoElement {
    let text: String = text.into();
    Button::new(id).small().outline().icon(IconName::Copy).label(label).on_click(move |_, window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        window.push_notification("Copied.", cx);
    })
}
