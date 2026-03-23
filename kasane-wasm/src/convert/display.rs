use kasane_core::display::DisplayDirective;

use crate::bindings::kasane::plugin::types as wit;

pub(crate) fn wit_display_directive_to_directive(
    directive: &wit::DisplayDirective,
) -> DisplayDirective {
    match directive {
        wit::DisplayDirective::Fold(fold) => DisplayDirective::Fold {
            range: fold.range_start as usize..fold.range_end as usize,
            summary: super::wit_atoms_to_atoms(&fold.summary),
        },
        wit::DisplayDirective::InsertAfter(insert) => DisplayDirective::InsertAfter {
            after: insert.after as usize,
            content: super::wit_atoms_to_atoms(&insert.content),
        },
        wit::DisplayDirective::InsertBefore(insert) => DisplayDirective::InsertBefore {
            before: insert.before as usize,
            content: super::wit_atoms_to_atoms(&insert.content),
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
        DisplayDirective::Fold { range, summary } => {
            wit::DisplayDirective::Fold(wit::FoldDirective {
                range_start: range.start as u32,
                range_end: range.end as u32,
                summary: super::atoms_to_wit(summary),
            })
        }
        DisplayDirective::InsertAfter { after, content } => {
            wit::DisplayDirective::InsertAfter(wit::InsertAfterDirective {
                after: *after as u32,
                content: super::atoms_to_wit(content),
            })
        }
        DisplayDirective::InsertBefore { before, content } => {
            wit::DisplayDirective::InsertBefore(wit::InsertBeforeDirective {
                before: *before as u32,
                content: super::atoms_to_wit(content),
            })
        }
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
