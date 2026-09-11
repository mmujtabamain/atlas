//! The honest empty state for a section whose milestone has not landed:
//! which milestone, which board task, which plan sections, what will appear.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use super::Section;

pub fn render(section: Section, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let milestone = section.pending_milestone();
    v_flex()
        .id(SharedString::from(format!("screen-{}", section.slug())))
        .test_support()
        .gap_6()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(section.label()))
                .child(div().text_sm().text_color(theme.muted_foreground).child(format!("Implements plan.md {}", section.plan_sections()))),
        )
        .child(
            v_flex()
                .gap_3()
                .py_8()
                .items_center()
                .text_color(theme.muted_foreground)
                .child(Icon::new(section.icon()).large())
                .child(match milestone {
                    Some(m) => format!("Arrives with milestone M{} — {} (board task #{})", m.number, m.title, m.task_id),
                    None => "Not planned".to_string(),
                })
                .child(
                    v_flex().gap_1().text_sm().children(section.promise().iter().map(|line| {
                        h_flex().gap_2().child("•").child(line.to_string())
                    })),
                ),
        )
}
