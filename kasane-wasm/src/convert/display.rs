use kasane_core::display::DisplayDirective;

use crate::bindings::kasane::plugin::types as wit;

#[cfg(test)]
use super::face_to_wit;
use super::wit_face_to_face;

pub(crate) fn wit_display_directive_to_directive(
    directive: &wit::DisplayDirective,
) -> DisplayDirective {
    match directive {
        wit::DisplayDirective::Fold(fold) => DisplayDirective::Fold {
            range: fold.range_start as usize..fold.range_end as usize,
            summary: fold.summary.clone(),
            face: wit_face_to_face(&fold.face),
        },
        wit::DisplayDirective::InsertAfter(insert) => DisplayDirective::InsertAfter {
            after: insert.after as usize,
            content: insert.content.clone(),
            face: wit_face_to_face(&insert.face),
        },
        wit::DisplayDirective::Hide(hide) => DisplayDirective::Hide {
            range: hide.range_start as usize..hide.range_end as usize,
        },
    }
}

pub(crate) fn wit_display_directives_to_directives(
    directives: &[wit::DisplayDirective],
) -> Vec<DisplayDirective> {
    directives
        .iter()
        .map(wit_display_directive_to_directive)
        .collect()
}

#[cfg(test)]
pub(crate) fn display_directive_to_wit(directive: &DisplayDirective) -> wit::DisplayDirective {
    match directive {
        DisplayDirective::Fold {
            range,
            summary,
            face,
        } => wit::DisplayDirective::Fold(wit::FoldDirective {
            range_start: range.start as u32,
            range_end: range.end as u32,
            summary: summary.clone(),
            face: face_to_wit(face),
        }),
        DisplayDirective::InsertAfter {
            after,
            content,
            face,
        } => wit::DisplayDirective::InsertAfter(wit::InsertAfterDirective {
            after: *after as u32,
            content: content.clone(),
            face: face_to_wit(face),
        }),
        DisplayDirective::Hide { range } => wit::DisplayDirective::Hide(wit::HideDirective {
            range_start: range.start as u32,
            range_end: range.end as u32,
        }),
    }
}

#[cfg(test)]
pub(crate) fn display_directives_to_wit(
    directives: &[DisplayDirective],
) -> Vec<wit::DisplayDirective> {
    directives.iter().map(display_directive_to_wit).collect()
}
