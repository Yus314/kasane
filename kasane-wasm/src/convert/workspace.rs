use kasane_core::workspace::WorkspaceQuery;

use crate::bindings::kasane::plugin::types as wit;

pub(crate) fn workspace_query_to_snapshot(query: &WorkspaceQuery<'_>) -> wit::WorkspaceSnapshot {
    let surfaces = query.surfaces();
    let rects = surfaces
        .iter()
        .filter_map(|surface_id| {
            query.rect_of(*surface_id).map(|rect| wit::SurfaceRect {
                surface_id: surface_id.0,
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: rect.h,
            })
        })
        .collect();

    wit::WorkspaceSnapshot {
        surfaces: surfaces.iter().map(|surface_id| surface_id.0).collect(),
        focused: query.focused().0,
        surface_count: query.surface_count() as u32,
        rects,
    }
}
