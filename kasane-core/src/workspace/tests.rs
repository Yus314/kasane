use super::*;

use crate::test_support::TestSurfaceBuilder;

#[test]
fn test_new_workspace() {
    let ws = Workspace::new(SurfaceId::BUFFER);
    assert_eq!(ws.focused(), SurfaceId::BUFFER);
    assert_eq!(ws.surface_count(), 1);
    assert!(ws.focus_history().is_empty());
}

#[test]
fn test_split_creates_two_surfaces() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    assert_eq!(ws.surface_count(), 2);
    let ids = ws.root().collect_ids();
    assert!(ids.contains(&SurfaceId::BUFFER));
    assert!(ids.contains(&new_id));
}

#[test]
fn test_split_tree_structure() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    match ws.root() {
        WorkspaceNode::Split {
            direction,
            ratio,
            first,
            second,
        } => {
            assert_eq!(*direction, SplitDirection::Vertical);
            assert_eq!(*ratio, 0.5);
            assert!(
                matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
            );
            assert!(
                matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == new_id)
            );
        }
        _ => panic!("expected Split"),
    }
}

#[test]
fn test_nested_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.split_focused(SplitDirection::Horizontal, 0.3);
    assert_eq!(ws.surface_count(), 3);
}

#[test]
fn test_focus_switch() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(new_id);
    assert_eq!(ws.focused(), new_id);
    assert_eq!(ws.focus_history(), &[SurfaceId::BUFFER]);
}

#[test]
fn test_focus_same_noop() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    ws.focus(SurfaceId::BUFFER);
    assert!(ws.focus_history().is_empty());
}

#[test]
fn test_focus_nonexistent_noop() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    ws.focus(SurfaceId(99));
    assert_eq!(ws.focused(), SurfaceId::BUFFER);
}

#[test]
fn test_focus_previous() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(new_id);
    ws.focus_previous();
    assert_eq!(ws.focused(), SurfaceId::BUFFER);
}

#[test]
fn test_focus_direction_right_uses_visible_geometry() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);

    let moved = ws.focus_direction(
        FocusDirection::Right,
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
    );

    assert_eq!(moved, Some(right));
    assert_eq!(ws.focused(), right);
}

#[test]
fn test_focus_direction_down_prefers_lower_neighbor() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let bottom = ws.split_focused(SplitDirection::Horizontal, 0.5);

    let moved = ws.focus_direction(
        FocusDirection::Down,
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        },
    );

    assert_eq!(moved, Some(bottom));
    assert_eq!(ws.focused(), bottom);
}

#[test]
fn test_focus_direction_next_cycles_visible_surfaces() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let second = ws.split_focused(SplitDirection::Vertical, 0.5);
    let third = ws.split_focused(SplitDirection::Horizontal, 0.5);

    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };

    assert_eq!(
        ws.focus_direction(FocusDirection::Next, total),
        Some(second)
    );
    assert_eq!(ws.focus_direction(FocusDirection::Next, total), Some(third));
    assert_eq!(
        ws.focus_direction(FocusDirection::Prev, total),
        Some(second)
    );
    assert_eq!(ws.focused(), second);
    assert_ne!(second, third);
}

#[test]
fn test_dispatch_workspace_command_with_total_handles_focus_direction() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
    let right = SurfaceId(10);
    reg.try_register(TestSurfaceBuilder::new(right).key("test.right").build())
        .unwrap();

    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: right,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    dirty = DirtyFlags::empty();
    dispatch_workspace_command_with_total(
        &mut reg,
        WorkspaceCommand::FocusDirection(FocusDirection::Right),
        &mut dirty,
        Some(Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        }),
    );

    assert_eq!(reg.workspace().focused(), right);
    assert!(dirty.contains(DirtyFlags::ALL));
}

#[test]
fn test_resize_focused_grows_first_child_ratio() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(SurfaceId::BUFFER);

    assert!(ws.resize_focused(0.1));

    match ws.root() {
        WorkspaceNode::Split { ratio, .. } => {
            assert!((*ratio - 0.6).abs() < f32::EPSILON);
        }
        other => panic!("expected Split root, got {other:?}"),
    }
    assert_ne!(right, SurfaceId::BUFFER);
}

#[test]
fn test_resize_focused_grows_second_child_by_reducing_ratio() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(right);

    assert!(ws.resize_focused(0.1));

    match ws.root() {
        WorkspaceNode::Split { ratio, .. } => {
            assert!((*ratio - 0.4).abs() < f32::EPSILON);
        }
        other => panic!("expected Split root, got {other:?}"),
    }
}

#[test]
fn test_resize_focused_targets_nearest_ancestor_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(right);
    let bottom_right = ws.split_focused(SplitDirection::Horizontal, 0.5);
    ws.focus(bottom_right);

    assert!(ws.resize_focused(0.1));

    match ws.root() {
        WorkspaceNode::Split { ratio, second, .. } => {
            assert!((*ratio - 0.5).abs() < f32::EPSILON);
            match second.as_ref() {
                WorkspaceNode::Split { ratio, .. } => {
                    assert!((*ratio - 0.4).abs() < f32::EPSILON);
                }
                other => panic!("expected nested Split, got {other:?}"),
            }
        }
        other => panic!("expected Split root, got {other:?}"),
    }
}

#[test]
fn test_resize_focused_returns_false_without_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    assert!(!ws.resize_focused(0.1));
}

#[test]
fn test_dispatch_workspace_command_resize_marks_dirty() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
    let right = SurfaceId(10);
    reg.try_register(TestSurfaceBuilder::new(right).key("test.right").build())
        .unwrap();

    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: right,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );
    reg.workspace_mut().focus(right);

    dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Resize { delta: 0.1 },
        &mut dirty,
    );

    assert!(dirty.contains(DirtyFlags::ALL));
    match reg.workspace().root() {
        WorkspaceNode::Split { ratio, .. } => {
            assert!((*ratio - 0.4).abs() < f32::EPSILON);
        }
        other => panic!("expected Split root, got {other:?}"),
    }
}

#[test]
fn test_swap_surfaces_exchanges_split_leaf_positions() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);

    assert!(ws.swap_surfaces(SurfaceId::BUFFER, right));

    match ws.root() {
        WorkspaceNode::Split { first, second, .. } => {
            assert!(
                matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == right)
            );
            assert!(
                matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
            );
        }
        other => panic!("expected Split root, got {other:?}"),
    }
    assert_eq!(ws.focused(), SurfaceId::BUFFER);
}

#[test]
fn test_swap_surfaces_exchanges_tiled_and_floating_positions() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let floated = ws.split_focused(SplitDirection::Vertical, 0.5);
    let float_rect = Rect {
        x: 12,
        y: 6,
        w: 24,
        h: 7,
    };
    assert!(ws.float_surface(floated, float_rect));

    assert!(ws.swap_surfaces(SurfaceId::BUFFER, floated));

    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&floated],
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24
        }
    );
    assert_eq!(rects[&SurfaceId::BUFFER], float_rect);
}

#[test]
fn test_swap_surfaces_returns_false_for_missing_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);
    assert!(!ws.swap_surfaces(right, SurfaceId(999)));
}

#[test]
fn test_dispatch_workspace_command_swap_marks_dirty() {
    let mut reg = SurfaceRegistry::new();
    reg.register(Box::new(crate::surface::buffer::KakouneBufferSurface::new()));
    let right = SurfaceId(10);
    reg.try_register(TestSurfaceBuilder::new(right).key("test.right").build())
        .unwrap();

    let mut dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::AddSurface {
            surface_id: right,
            placement: Placement::SplitFocused {
                direction: SplitDirection::Vertical,
                ratio: 0.5,
            },
        },
        &mut dirty,
    );

    dirty = DirtyFlags::empty();
    dispatch_workspace_command(
        &mut reg,
        WorkspaceCommand::Swap(SurfaceId::BUFFER, right),
        &mut dirty,
    );

    assert!(dirty.contains(DirtyFlags::ALL));
    match reg.workspace().root() {
        WorkspaceNode::Split { first, second, .. } => {
            assert!(
                matches!(first.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == right)
            );
            assert!(
                matches!(second.as_ref(), WorkspaceNode::Leaf { surface_id } if *surface_id == SurfaceId::BUFFER)
            );
        }
        other => panic!("expected Split root, got {other:?}"),
    }
}

#[test]
fn test_close_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    assert!(ws.close(new_id));
    assert_eq!(ws.surface_count(), 1);
}

#[test]
fn test_close_focused_switches_focus() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(new_id);
    assert!(ws.close(new_id));
    assert_eq!(ws.focused(), SurfaceId::BUFFER);
}

#[test]
fn test_cannot_close_last_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    assert!(!ws.close(SurfaceId::BUFFER));
}

#[test]
fn test_close_nonexistent() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    assert!(!ws.close(SurfaceId(99)));
}

#[test]
fn test_compute_rects_single() {
    let ws = Workspace::new(SurfaceId::BUFFER);
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let rects = ws.compute_rects(total);
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[&SurfaceId::BUFFER], total);
}

#[test]
fn test_compute_rects_vertical_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 81,
        h: 24,
    };
    let rects = ws.compute_rects(total);
    assert_eq!(rects.len(), 2);
    let r0 = rects[&SurfaceId::BUFFER];
    let r1 = rects[&new_id];
    assert_eq!(r0.x, 0);
    assert_eq!(r0.w, 40);
    assert_eq!(r1.x, 41);
    assert_eq!(r1.w, 40);
}

#[test]
fn test_compute_rects_horizontal_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Horizontal, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 25,
    };
    let rects = ws.compute_rects(total);
    let r0 = rects[&SurfaceId::BUFFER];
    let r1 = rects[&new_id];
    assert_eq!(r0.y, 0);
    assert_eq!(r0.h, 12);
    assert_eq!(r1.y, 13);
    assert_eq!(r1.h, 12);
}

#[test]
fn test_surface_at() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 81,
        h: 24,
    };
    assert_eq!(ws.surface_at(0, 0, total), Some(SurfaceId::BUFFER));
    assert_eq!(ws.surface_at(39, 12, total), Some(SurfaceId::BUFFER));
    assert_eq!(ws.surface_at(41, 0, total), Some(new_id));
    assert_eq!(ws.surface_at(80, 23, total), Some(new_id));
    assert_eq!(ws.surface_at(40, 12, total), None); // divider
}

#[test]
fn test_compute_dividers_vertical_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    ws.split_focused(SplitDirection::Vertical, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };

    let dividers = ws.compute_dividers(total);
    assert_eq!(dividers.len(), 1);
    assert_eq!(dividers[0].id, WorkspaceDividerId(0));
    assert_eq!(dividers[0].direction, SplitDirection::Vertical);
    assert_eq!(
        dividers[0].rect,
        Rect {
            x: 40,
            y: 0,
            w: 1,
            h: 24,
        }
    );
    assert_eq!(dividers[0].available_main, 79);
    assert_eq!(ws.divider_at(40, 12, total), Some(dividers[0]));
}

#[test]
fn test_set_divider_ratio_updates_exact_split() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.5);
    ws.focus(right);
    ws.split_focused(SplitDirection::Horizontal, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };

    let dividers = ws.compute_dividers(total);
    let horizontal = dividers
        .iter()
        .find(|divider| divider.direction == SplitDirection::Horizontal)
        .copied()
        .expect("expected nested horizontal divider");
    assert!(ws.set_divider_ratio(horizontal.id, 0.25));

    match ws.root() {
        WorkspaceNode::Split { ratio, second, .. } => {
            assert_eq!(*ratio, 0.5, "root split should remain unchanged");
            match second.as_ref() {
                WorkspaceNode::Split { ratio, .. } => {
                    assert!((*ratio - 0.25).abs() < f32::EPSILON);
                }
                other => panic!("expected nested split, got {other:?}"),
            }
        }
        other => panic!("expected root split, got {other:?}"),
    }
}

#[test]
fn test_tabs_node() {
    let node = WorkspaceNode::Tabs {
        tabs: vec![
            WorkspaceNode::leaf(SurfaceId(10)),
            WorkspaceNode::leaf(SurfaceId(11)),
        ],
        active: 0,
        labels: vec!["tab1".into(), "tab2".into()],
    };
    assert_eq!(node.leaf_count(), 2);
    assert!(node.find(SurfaceId(10)).is_some());
    assert!(node.find(SurfaceId(11)).is_some());
}

#[test]
fn test_tabs_remove_collapses() {
    let mut node = WorkspaceNode::Tabs {
        tabs: vec![
            WorkspaceNode::leaf(SurfaceId(10)),
            WorkspaceNode::leaf(SurfaceId(11)),
        ],
        active: 0,
        labels: vec!["tab1".into(), "tab2".into()],
    };
    assert!(node.remove(SurfaceId(10)));
    assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(11)));
}

#[test]
fn test_tabs_compute_rects() {
    let node = WorkspaceNode::Tabs {
        tabs: vec![
            WorkspaceNode::leaf(SurfaceId(10)),
            WorkspaceNode::leaf(SurfaceId(11)),
        ],
        active: 0,
        labels: vec!["tab1".into(), "tab2".into()],
    };
    let area = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let rects = node.compute_rects(area);
    assert_eq!(rects.len(), 1);
    let r = rects[&SurfaceId(10)];
    assert_eq!(r.y, 1);
    assert_eq!(r.h, 23);
}

#[test]
fn test_add_tab_wraps_leaf_and_focuses_new_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    assert!(ws.add_tab(SurfaceId::BUFFER, SurfaceId(10), "buffer", "plugin.tab"));
    assert_eq!(ws.focused(), SurfaceId(10));
    match ws.root() {
        WorkspaceNode::Tabs {
            tabs,
            active,
            labels,
        } => {
            assert_eq!(*active, 1);
            assert_eq!(tabs.len(), 2);
            assert_eq!(
                labels,
                &vec!["buffer".to_string(), "plugin.tab".to_string()]
            );
        }
        other => panic!("expected Tabs root, got {other:?}"),
    }
}

#[test]
fn test_float_node() {
    let node = WorkspaceNode::Float {
        base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
        floating: vec![FloatingEntry {
            node: WorkspaceNode::leaf(SurfaceId(10)),
            rect: Rect {
                x: 10,
                y: 5,
                w: 30,
                h: 10,
            },
            z_order: 0,
            restore: None,
        }],
    };
    assert_eq!(node.leaf_count(), 2);
    assert!(node.find(SurfaceId(0)).is_some());
    assert!(node.find(SurfaceId(10)).is_some());
}

#[test]
fn test_float_compute_rects() {
    let node = WorkspaceNode::Float {
        base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
        floating: vec![FloatingEntry {
            node: WorkspaceNode::leaf(SurfaceId(10)),
            rect: Rect {
                x: 10,
                y: 5,
                w: 30,
                h: 10,
            },
            z_order: 0,
            restore: None,
        }],
    };
    let total = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };
    let rects = node.compute_rects(total);
    assert_eq!(rects[&SurfaceId(0)], total);
    assert_eq!(
        rects[&SurfaceId(10)],
        Rect {
            x: 10,
            y: 5,
            w: 30,
            h: 10
        }
    );
}

#[test]
fn test_float_remove_floating() {
    let mut node = WorkspaceNode::Float {
        base: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
        floating: vec![FloatingEntry {
            node: WorkspaceNode::leaf(SurfaceId(10)),
            rect: Rect {
                x: 10,
                y: 5,
                w: 30,
                h: 10,
            },
            z_order: 0,
            restore: None,
        }],
    };
    assert!(node.remove(SurfaceId(10)));
    // Floating should be empty now
    if let WorkspaceNode::Float { floating, .. } = &node {
        assert!(floating.is_empty());
    } else {
        panic!("expected Float");
    }
}

#[test]
fn test_dock_surface_left_wraps_root() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    ws.dock_surface(SurfaceId(10), DockPosition::Left, 0.25);
    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId(10)],
        Rect {
            x: 0,
            y: 0,
            w: 20,
            h: 24,
        }
    );
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 21,
            y: 0,
            w: 59,
            h: 24,
        }
    );
}

#[test]
fn test_add_floating_wraps_root_and_assigns_rect() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let float_rect = Rect {
        x: 10,
        y: 5,
        w: 30,
        h: 8,
    };
    ws.add_floating(SurfaceId(20), float_rect);
    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24
        }
    );
    assert_eq!(rects[&SurfaceId(20)], float_rect);
}

#[test]
fn test_float_surface_moves_existing_leaf_into_floating_layer() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let floated = ws.split_focused(SplitDirection::Vertical, 0.5);
    let float_rect = Rect {
        x: 12,
        y: 6,
        w: 24,
        h: 7,
    };

    assert!(ws.float_surface(floated, float_rect));

    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 80,
            h: 24,
        }
    );
    assert_eq!(rects[&floated], float_rect);
}

#[test]
fn test_float_surface_rejects_last_tiled_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    assert!(!ws.float_surface(
        SurfaceId::BUFFER,
        Rect {
            x: 2,
            y: 2,
            w: 10,
            h: 5,
        }
    ));
}

#[test]
fn test_unfloat_surface_retiles_floating_entry() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let floated = ws.split_focused(SplitDirection::Vertical, 0.3);
    assert!(ws.float_surface(
        floated,
        Rect {
            x: 12,
            y: 6,
            w: 24,
            h: 7,
        }
    ));

    assert!(ws.unfloat_surface(floated, SplitDirection::Horizontal, 0.8));
    assert_eq!(ws.focused(), floated);

    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 24,
            h: 24,
        }
    );
    assert_eq!(
        rects[&floated],
        Rect {
            x: 25,
            y: 0,
            w: 55,
            h: 24,
        }
    );
}

#[test]
fn test_unfloat_surface_restores_first_side_from_saved_placement() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let right = ws.split_focused(SplitDirection::Vertical, 0.3);
    assert!(ws.float_surface(
        SurfaceId::BUFFER,
        Rect {
            x: 2,
            y: 2,
            w: 10,
            h: 5,
        }
    ));

    assert!(ws.unfloat_surface(SurfaceId::BUFFER, SplitDirection::Horizontal, 0.8));

    let rects = ws.compute_rects(Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    });
    assert_eq!(
        rects[&SurfaceId::BUFFER],
        Rect {
            x: 0,
            y: 0,
            w: 24,
            h: 24,
        }
    );
    assert_eq!(
        rects[&right],
        Rect {
            x: 25,
            y: 0,
            w: 55,
            h: 24,
        }
    );
}

#[test]
fn test_unfloat_surface_returns_false_for_non_floating_surface() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let tiled = ws.split_focused(SplitDirection::Vertical, 0.5);
    assert!(!ws.unfloat_surface(tiled, SplitDirection::Vertical, 0.5));
}

#[test]
fn test_next_surface_id() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let id1 = ws.next_surface_id();
    let id2 = ws.next_surface_id();
    assert_eq!(id1.0, SurfaceId::PLUGIN_BASE);
    assert_eq!(id2.0, SurfaceId::PLUGIN_BASE + 1);
}

#[test]
fn test_close_cleans_focus_history() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let id1 = ws.split_focused(SplitDirection::Vertical, 0.5);
    let id2 = ws.split_focused(SplitDirection::Horizontal, 0.5);
    ws.focus(id1);
    ws.focus(id2);
    ws.close(id1);
    assert!(!ws.focus_history().contains(&id1));
}

#[test]
fn test_workspace_query() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    let total = Rect {
        x: 0,
        y: 0,
        w: 81,
        h: 24,
    };
    let query = ws.query(total);
    assert_eq!(query.surface_count(), 2);
    assert_eq!(query.focused(), SurfaceId::BUFFER);
    assert!(query.rect_of(SurfaceId::BUFFER).is_some());
    assert!(query.rect_of(new_id).is_some());
    assert!(query.rect_of(SurfaceId(99)).is_none());
    assert_eq!(query.surfaces().len(), 2);
}

#[test]
fn test_node_find() {
    let mut ws = Workspace::new(SurfaceId::BUFFER);
    let new_id = ws.split_focused(SplitDirection::Vertical, 0.5);
    assert!(ws.root().find(SurfaceId::BUFFER).is_some());
    assert!(ws.root().find(new_id).is_some());
    assert!(ws.root().find(SurfaceId(99)).is_none());
}

#[test]
fn test_node_remove_first() {
    let mut node = WorkspaceNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        first: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
        second: Box::new(WorkspaceNode::leaf(SurfaceId(1))),
    };
    assert!(node.remove(SurfaceId(0)));
    assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(1)));
}

#[test]
fn test_node_remove_second() {
    let mut node = WorkspaceNode::Split {
        direction: SplitDirection::Vertical,
        ratio: 0.5,
        first: Box::new(WorkspaceNode::leaf(SurfaceId(0))),
        second: Box::new(WorkspaceNode::leaf(SurfaceId(1))),
    };
    assert!(node.remove(SurfaceId(1)));
    assert!(matches!(node, WorkspaceNode::Leaf { surface_id } if surface_id == SurfaceId(0)));
}

#[test]
fn test_split_rect_vertical() {
    let area = Rect {
        x: 0,
        y: 0,
        w: 81,
        h: 24,
    };
    let (a, b) = area.split(SplitDirection::Vertical, 0.5);
    assert_eq!(a.w, 40);
    assert_eq!(b.w, 40);
    assert_eq!(a.x, 0);
    assert_eq!(b.x, 41);
}

#[test]
fn test_split_rect_horizontal() {
    let area = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 25,
    };
    let (a, b) = area.split(SplitDirection::Horizontal, 0.5);
    assert_eq!(a.h, 12);
    assert_eq!(b.h, 12);
    assert_eq!(a.y, 0);
    assert_eq!(b.y, 13);
}
