use crate::element::BorderLineStyle;
use crate::layout::Rect;
use crate::plugin::{FaceMerge, SurfaceOrn, SurfaceOrnAnchor, SurfaceOrnKind};
use crate::protocol::Face;
use crate::render::grid::CellGrid;
use crate::render::scene::{CellSize, DrawCommand, to_pixel_rect};
use crate::surface::{SurfaceId, SurfaceRegistry};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedSurfaceOrn {
    pub surface_id: Option<SurfaceId>,
    pub rect: Rect,
    pub kind: SurfaceOrnKind,
    pub face: Face,
}

#[derive(Debug, Clone)]
struct SurfaceCandidate {
    score: (i8, i16),
    resolved: ResolvedSurfaceOrn,
}

fn kind_order(kind: SurfaceOrnKind) -> u8 {
    match kind {
        SurfaceOrnKind::InactiveTint => 0,
        SurfaceOrnKind::FocusFrame => 1,
    }
}

fn upsert_surface_candidate(winners: &mut Vec<SurfaceCandidate>, candidate: SurfaceCandidate) {
    if let Some(existing) = winners.iter_mut().find(|winner| {
        winner.resolved.surface_id == candidate.resolved.surface_id
            && winner.resolved.kind == candidate.resolved.kind
    }) {
        if candidate.score > existing.score {
            *existing = candidate;
        }
        return;
    }
    winners.push(candidate);
}

pub(crate) fn resolve_surface_ornaments(
    surfaces: &[SurfaceOrn],
    surface_registry: Option<&SurfaceRegistry>,
    focused_pane_rect: Option<Rect>,
    total: Rect,
) -> Vec<ResolvedSurfaceOrn> {
    let workspace_rects =
        surface_registry.map(|registry| registry.workspace().compute_rects(total));
    let focused_surface_id = surface_registry.map(|registry| registry.workspace().focused());
    let focused_surface_rect = focused_pane_rect.or_else(|| {
        workspace_rects.as_ref().and_then(|rects| {
            focused_surface_id.and_then(|surface_id| rects.get(&surface_id).copied())
        })
    });

    let mut winners = Vec::new();
    for orn in surfaces {
        let score = (orn.modality.rank(), orn.priority);
        let resolved = match &orn.anchor {
            SurfaceOrnAnchor::FocusedSurface => {
                let Some(rect) = focused_surface_rect else {
                    continue;
                };
                if orn.kind != SurfaceOrnKind::FocusFrame {
                    continue;
                }
                ResolvedSurfaceOrn {
                    surface_id: focused_surface_id,
                    rect,
                    kind: orn.kind,
                    face: orn.face,
                }
            }
            SurfaceOrnAnchor::SurfaceKey(surface_key) => {
                let Some(registry) = surface_registry else {
                    continue;
                };
                let Some(surface_id) = registry.surface_id_by_key(surface_key) else {
                    continue;
                };
                let Some(rect) = workspace_rects
                    .as_ref()
                    .and_then(|rects| rects.get(&surface_id).copied())
                else {
                    continue;
                };
                let is_focused = Some(surface_id) == focused_surface_id;
                match orn.kind {
                    SurfaceOrnKind::FocusFrame if !is_focused => continue,
                    SurfaceOrnKind::InactiveTint if is_focused => continue,
                    _ => {}
                }
                ResolvedSurfaceOrn {
                    surface_id: Some(surface_id),
                    rect,
                    kind: orn.kind,
                    face: orn.face,
                }
            }
        };
        upsert_surface_candidate(&mut winners, SurfaceCandidate { score, resolved });
    }

    let mut resolved: Vec<_> = winners.into_iter().map(|winner| winner.resolved).collect();
    resolved.sort_by_key(|item| {
        (
            kind_order(item.kind),
            item.surface_id.map(|id| id.0).unwrap_or(u32::MAX),
        )
    });
    resolved
}

pub(crate) fn apply_surface_ornaments_tui(grid: &mut CellGrid, ornaments: &[ResolvedSurfaceOrn]) {
    for orn in ornaments {
        match orn.kind {
            SurfaceOrnKind::InactiveTint => {
                apply_rect_face(grid, &orn.rect, &orn.face, FaceMerge::Background)
            }
            SurfaceOrnKind::FocusFrame => {
                apply_rect_perimeter_face(grid, &orn.rect, &orn.face, FaceMerge::Overlay)
            }
        }
    }
}

pub(crate) fn lower_surface_ornaments_gui(
    ornaments: &[ResolvedSurfaceOrn],
    cell_size: CellSize,
) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    for orn in ornaments {
        match orn.kind {
            SurfaceOrnKind::InactiveTint => commands.push(DrawCommand::FillRect {
                rect: to_pixel_rect(&orn.rect, cell_size),
                face: orn.face.into(),
                elevated: false,
            }),
            SurfaceOrnKind::FocusFrame => commands.push(DrawCommand::DrawBorder {
                rect: to_pixel_rect(&orn.rect, cell_size),
                line_style: BorderLineStyle::Single,
                face: orn.face.into(),
                fill_face: None,
            }),
        }
    }
    commands
}

fn apply_rect_face(grid: &mut CellGrid, rect: &Rect, face: &Face, merge: FaceMerge) {
    let style = crate::protocol::Style::from_face(face);
    let x_end = rect.x.saturating_add(rect.w).min(grid.width());
    let y_end = rect.y.saturating_add(rect.h).min(grid.height());
    for y in rect.y..y_end {
        for x in rect.x..x_end {
            if let Some(cell) = grid.get_mut(x, y) {
                cell.with_style_mut(|s| merge.apply_to_terminal(s, &style));
            }
        }
    }
}

fn apply_rect_perimeter_face(grid: &mut CellGrid, rect: &Rect, face: &Face, merge: FaceMerge) {
    let style = crate::protocol::Style::from_face(face);
    let x_end = rect.x.saturating_add(rect.w).min(grid.width());
    let y_end = rect.y.saturating_add(rect.h).min(grid.height());
    if rect.w == 0 || rect.h == 0 || rect.x >= x_end || rect.y >= y_end {
        return;
    }

    // Top row
    for x in rect.x..x_end {
        if let Some(cell) = grid.get_mut(x, rect.y) {
            cell.with_style_mut(|s| merge.apply_to_terminal(s, &style));
        }
    }
    // Bottom row (skip if same as top)
    let bottom = y_end - 1;
    if bottom != rect.y {
        for x in rect.x..x_end {
            if let Some(cell) = grid.get_mut(x, bottom) {
                cell.with_style_mut(|s| merge.apply_to_terminal(s, &style));
            }
        }
    }
    // Left and right columns (excluding corners already covered)
    for y in (rect.y + 1)..bottom {
        if let Some(cell) = grid.get_mut(rect.x, y) {
            cell.with_style_mut(|s| merge.apply_to_terminal(s, &style));
        }
        let right = x_end - 1;
        if right != rect.x
            && let Some(cell) = grid.get_mut(right, y)
        {
            cell.with_style_mut(|s| merge.apply_to_terminal(s, &style));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::SplitDirection;
    use crate::plugin::{OrnamentModality, SurfaceOrn};
    use crate::protocol::{Color, NamedColor};
    use crate::surface::SurfaceRegistry;
    use crate::surface::buffer::KakouneBufferSurface;
    use crate::surface::status::StatusBarSurface;
    use crate::test_support::TestSurfaceBuilder;

    fn face(bg: NamedColor) -> Face {
        Face {
            bg: Color::Named(bg),
            ..Face::default()
        }
    }

    #[test]
    fn resolve_surface_ornaments_prefers_higher_priority_per_surface_and_kind() {
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let mut registry = SurfaceRegistry::new();
        registry
            .try_register(Box::new(KakouneBufferSurface::new()))
            .unwrap();
        registry
            .try_register(Box::new(StatusBarSurface::new()))
            .unwrap();
        registry
            .try_register(
                TestSurfaceBuilder::new(SurfaceId(200))
                    .key("test.right")
                    .build(),
            )
            .unwrap();
        registry.workspace_mut().root_mut().split(
            SurfaceId::BUFFER,
            SplitDirection::Vertical,
            0.5,
            SurfaceId(200),
        );
        registry.workspace_mut().focus(SurfaceId::BUFFER);

        let surfaces = vec![
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::FocusedSurface,
                kind: SurfaceOrnKind::FocusFrame,
                face: face(NamedColor::Blue),
                priority: 1,
                modality: OrnamentModality::Approximate,
            },
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::FocusedSurface,
                kind: SurfaceOrnKind::FocusFrame,
                face: face(NamedColor::Red),
                priority: 5,
                modality: OrnamentModality::Must,
            },
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::SurfaceKey("test.right".into()),
                kind: SurfaceOrnKind::InactiveTint,
                face: face(NamedColor::Yellow),
                priority: 2,
                modality: OrnamentModality::Approximate,
            },
        ];

        let resolved = resolve_surface_ornaments(&surfaces, Some(&registry), None, total);
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0].kind, SurfaceOrnKind::InactiveTint);
        assert_eq!(resolved[1].kind, SurfaceOrnKind::FocusFrame);
        assert_eq!(resolved[1].face.bg, Color::Named(NamedColor::Red));
        assert_eq!(resolved[1].surface_id, Some(SurfaceId::BUFFER));
        assert_eq!(resolved[0].surface_id, Some(SurfaceId(200)));
    }

    #[test]
    fn resolve_surface_ornaments_enforces_focus_truth() {
        let total = Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        };
        let mut registry = SurfaceRegistry::new();
        registry
            .try_register(Box::new(KakouneBufferSurface::new()))
            .unwrap();
        registry
            .try_register(Box::new(StatusBarSurface::new()))
            .unwrap();
        registry
            .try_register(
                TestSurfaceBuilder::new(SurfaceId(200))
                    .key("test.right")
                    .build(),
            )
            .unwrap();
        registry.workspace_mut().root_mut().split(
            SurfaceId::BUFFER,
            SplitDirection::Vertical,
            0.5,
            SurfaceId(200),
        );
        registry.workspace_mut().focus(SurfaceId(200));

        let surfaces = vec![
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::FocusedSurface,
                kind: SurfaceOrnKind::InactiveTint,
                face: face(NamedColor::Yellow),
                priority: 1,
                modality: OrnamentModality::Approximate,
            },
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::SurfaceKey("kasane.buffer".into()),
                kind: SurfaceOrnKind::FocusFrame,
                face: face(NamedColor::Blue),
                priority: 1,
                modality: OrnamentModality::Approximate,
            },
            SurfaceOrn {
                anchor: SurfaceOrnAnchor::SurfaceKey("kasane.buffer".into()),
                kind: SurfaceOrnKind::InactiveTint,
                face: face(NamedColor::Red),
                priority: 1,
                modality: OrnamentModality::Approximate,
            },
        ];

        let resolved = resolve_surface_ornaments(&surfaces, Some(&registry), None, total);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].surface_id, Some(SurfaceId::BUFFER));
        assert_eq!(resolved[0].kind, SurfaceOrnKind::InactiveTint);
    }

    #[test]
    fn apply_surface_ornaments_tui_tints_interior_and_frames_perimeter() {
        let mut grid = CellGrid::new(6, 4);
        let default_face = Face {
            bg: Color::Named(NamedColor::Black),
            ..Face::default()
        };
        grid.clear(&default_face);

        let ornaments = vec![
            ResolvedSurfaceOrn {
                surface_id: Some(SurfaceId(1)),
                rect: Rect {
                    x: 1,
                    y: 1,
                    w: 3,
                    h: 2,
                },
                kind: SurfaceOrnKind::InactiveTint,
                face: face(NamedColor::Blue),
            },
            ResolvedSurfaceOrn {
                surface_id: Some(SurfaceId(2)),
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: 5,
                    h: 4,
                },
                kind: SurfaceOrnKind::FocusFrame,
                face: face(NamedColor::Red),
            },
        ];

        apply_surface_ornaments_tui(&mut grid, &ornaments);

        assert_eq!(
            grid.get(2, 2).unwrap().face().bg,
            Color::Named(NamedColor::Blue)
        );
        assert_eq!(
            grid.get(0, 0).unwrap().face().bg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            grid.get(4, 3).unwrap().face().bg,
            Color::Named(NamedColor::Red)
        );
        assert_eq!(
            grid.get(5, 3).unwrap().face().bg,
            Color::Named(NamedColor::Black)
        );
    }

    #[test]
    fn lower_surface_ornaments_gui_emits_fill_and_border_commands() {
        let commands = lower_surface_ornaments_gui(
            &[
                ResolvedSurfaceOrn {
                    surface_id: Some(SurfaceId(1)),
                    rect: Rect {
                        x: 1,
                        y: 2,
                        w: 3,
                        h: 4,
                    },
                    kind: SurfaceOrnKind::InactiveTint,
                    face: face(NamedColor::Blue),
                },
                ResolvedSurfaceOrn {
                    surface_id: Some(SurfaceId(2)),
                    rect: Rect {
                        x: 0,
                        y: 0,
                        w: 5,
                        h: 6,
                    },
                    kind: SurfaceOrnKind::FocusFrame,
                    face: face(NamedColor::Red),
                },
            ],
            CellSize {
                width: 10.0,
                height: 20.0,
            },
        );

        assert!(matches!(
            &commands[0],
            DrawCommand::FillRect {
                rect,
                elevated: false,
                ..
            } if rect.x == 10.0 && rect.y == 40.0 && rect.w == 30.0 && rect.h == 80.0
        ));
        assert!(matches!(
            &commands[1],
            DrawCommand::DrawBorder {
                rect,
                line_style: BorderLineStyle::Single,
                fill_face: None,
                ..
            } if rect.w == 50.0 && rect.h == 120.0
        ));
    }
}
