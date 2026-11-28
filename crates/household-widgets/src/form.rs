// SPDX-License-Identifier: GPL-3.0-only

//! Responsive labelled fields for dialogs and settings forms.

use cosmic::iced::widget::text::Wrapping;
use cosmic::{theme, widget};

use crate::layout::{self, Child, Level, ResponsiveLevels, Stage};

/// One labelled control. Supporting content follows its control in both
/// arrangements. Keep editable state and widget IDs in the caller.
pub struct Field<'a, M> {
    label: String,
    label_size: Option<f32>,
    control: Child<'a, M>,
    supporting: Vec<Child<'a, M>>,
}

impl<'a, M: 'a> Field<'a, M> {
    pub fn new(label: impl Into<String>, control: Child<'a, M>) -> Self {
        Self {
            label: label.into(),
            label_size: None,
            control,
            supporting: Vec::new(),
        }
    }

    /// Add help, validation, or a related action below the control.
    pub fn supporting(mut self, content: Child<'a, M>) -> Self {
        self.supporting.push(content);
        self
    }

    /// Set the label's text size in logical pixels.
    pub fn label_size(mut self, size: f32) -> Self {
        self.label_size = Some(size);
        self
    }
}

/// Lay out fields with a common label column when their preferred forms fit
/// beside it. Then try labels above their controls, first at the preferred
/// level and then at the mild level. Child controls declare their useful
/// widths and any mild forms. Put tall forms in the host's vertical scroll
/// region to let the form use its measured height.
pub fn fields<'a, M: 'a>(fields: Vec<Field<'a, M>>, label_width: f32) -> ResponsiveLevels<'a, M> {
    let spacing = theme::active().cosmic().spacing;
    let label_gap = f32::from(spacing.space_s);
    let support_gap = f32::from(spacing.space_xxxs);
    let mut aligned = Vec::with_capacity(fields.len());
    let mut stacked = Vec::with_capacity(fields.len());

    for field in fields {
        let label = layout::child(move |_| {
            let text = widget::text(field.label.clone()).wrapping(Wrapping::WordOrGlyph);
            match field.label_size {
                Some(size) => text.size(size).into(),
                None => text.into(),
            }
        })
        .minimum_width(|_, size| size.width.min(16.0));

        let mut content = vec![field.control];
        content.extend(field.supporting);
        let content = layout::column_child(content, support_gap).fill_width();
        aligned.push(layout::row_child_top(
            vec![
                label.clone().fixed_dimensions(Some(label_width), None),
                content.clone(),
            ],
            label_gap,
        ));
        stacked.push(layout::column_child(vec![label, content], support_gap));
    }

    let row_gap = f32::from(spacing.space_m);
    let stacked = layout::column_child(stacked, row_gap);
    layout::stages(vec![
        Stage::new(Level::Preferred, layout::column_child(aligned, row_gap)),
        Stage::new(Level::Preferred, stacked.clone()),
        Stage::new(Level::Mild, stacked),
    ])
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
