use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::element::{Element, FlexChild, InteractiveId};
use crate::layout::HitMap;
use crate::state::{AppState, DirtyFlags};

use super::{
    AnnotateContext, AnnotationResult, BackgroundLayer, Command, ContributeContext, Contribution,
    OverlayContext, OverlayContribution, PaintHook, Plugin, PluginCapabilities, PluginId, SlotId,
    TransformContext, TransformTarget, extract_redraw_flags,
};

/// Cached result for a single plugin's contributions.
#[derive(Default)]
struct PluginCacheEntry {
    last_state_hash: u64,
    /// Cached contribute_to() results, keyed by SlotId.
    contributions: HashMap<SlotId, Option<Contribution>>,
}

struct PluginSlotCache {
    entries: Vec<PluginCacheEntry>,
}

impl PluginSlotCache {
    fn new() -> Self {
        PluginSlotCache {
            entries: Vec::new(),
        }
    }
}

/// Effective DirtyFlags dependencies for each ViewCache section,
/// computed by unioning core deps with plugin contribution/transform/annotation deps.
#[derive(Debug, Clone, Copy)]
pub struct EffectiveSectionDeps {
    pub base: DirtyFlags,
    pub menu: DirtyFlags,
    pub info: DirtyFlags,
}

impl Default for EffectiveSectionDeps {
    fn default() -> Self {
        use crate::render::view::{
            BUILD_BASE_DEPS, BUILD_INFO_SECTION_DEPS, BUILD_MENU_SECTION_DEPS,
        };
        EffectiveSectionDeps {
            base: BUILD_BASE_DEPS,
            menu: BUILD_MENU_SECTION_DEPS,
            info: BUILD_INFO_SECTION_DEPS,
        }
    }
}

pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
    capabilities: Vec<PluginCapabilities>,
    hit_map: HitMap,
    slot_cache: RefCell<PluginSlotCache>,
    any_plugin_state_changed: bool,
    section_deps: EffectiveSectionDeps,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
            capabilities: Vec::new(),
            hit_map: HitMap::new(),
            slot_cache: RefCell::new(PluginSlotCache::new()),
            any_plugin_state_changed: false,
            section_deps: EffectiveSectionDeps::default(),
        }
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Check if any registered plugin has the given capability.
    fn has_capability(&self, cap: PluginCapabilities) -> bool {
        self.capabilities.iter().any(|c| c.contains(cap))
    }

    /// Returns true if any plugin's state_hash changed during the last
    /// `prepare_plugin_cache()` call.
    pub fn any_plugin_state_changed(&self) -> bool {
        self.any_plugin_state_changed
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.id();
        let caps = plugin.capabilities();
        if let Some(pos) = self.plugins.iter().position(|p| p.id() == id) {
            // Replace existing plugin with same ID (e.g. FS plugin overrides bundled)
            self.plugins[pos] = plugin;
            self.capabilities[pos] = caps;
            // Reset the cache entry for the replaced plugin
            self.slot_cache.get_mut().entries[pos] = PluginCacheEntry::default();
        } else {
            self.plugins.push(plugin);
            self.capabilities.push(caps);
            self.slot_cache
                .get_mut()
                .entries
                .push(PluginCacheEntry::default());
        }
        self.recompute_section_deps();
    }

    /// Recompute effective section deps by unioning core deps with all
    /// plugin contribution/transform/annotation deps.
    fn recompute_section_deps(&mut self) {
        use crate::render::view::{
            BUILD_BASE_DEPS, BUILD_INFO_SECTION_DEPS, BUILD_MENU_SECTION_DEPS,
        };

        let mut base = BUILD_BASE_DEPS;
        let mut menu = BUILD_MENU_SECTION_DEPS;
        let mut info = BUILD_INFO_SECTION_DEPS;

        // Base slots
        let base_slots = [
            &SlotId::BUFFER_LEFT,
            &SlotId::BUFFER_RIGHT,
            &SlotId::ABOVE_BUFFER,
            &SlotId::BELOW_BUFFER,
            &SlotId::ABOVE_STATUS,
            &SlotId::STATUS_LEFT,
            &SlotId::STATUS_RIGHT,
        ];

        for plugin in &self.plugins {
            // Contribution deps for base slots
            for slot in &base_slots {
                base |= plugin.contribute_deps(slot);
            }

            // Annotation deps
            base |= plugin.annotate_deps();

            // Transform deps for base targets
            base |= plugin.transform_deps(&TransformTarget::Buffer);
            base |= plugin.transform_deps(&TransformTarget::StatusBar);

            // Transform deps for menu targets
            menu |= plugin.transform_deps(&TransformTarget::Menu);
            menu |= plugin.transform_deps(&TransformTarget::MenuPrompt);
            menu |= plugin.transform_deps(&TransformTarget::MenuInline);
            menu |= plugin.transform_deps(&TransformTarget::MenuSearch);

            // Transform deps for info targets
            info |= plugin.transform_deps(&TransformTarget::Info);
            info |= plugin.transform_deps(&TransformTarget::InfoPrompt);
            info |= plugin.transform_deps(&TransformTarget::InfoModal);
        }

        self.section_deps = EffectiveSectionDeps { base, menu, info };
    }

    /// Get the effective section deps (includes plugin contributions).
    pub fn section_deps(&self) -> &EffectiveSectionDeps {
        &self.section_deps
    }

    /// Invalidate cache entries based on dirty flags and state hash changes.
    /// Call once per frame before rendering (during the mutable phase).
    pub fn prepare_plugin_cache(&mut self, dirty: DirtyFlags) {
        let cache = self.slot_cache.get_mut();
        self.any_plugin_state_changed = false;

        // Grow entries if plugins were registered after last prepare
        while cache.entries.len() < self.plugins.len() {
            cache.entries.push(PluginCacheEntry::default());
        }

        for (i, plugin) in self.plugins.iter().enumerate() {
            let entry = &mut cache.entries[i];
            let current_hash = plugin.state_hash();

            // L1: state hash changed → invalidate all contributions for this plugin
            if current_hash != entry.last_state_hash {
                entry.last_state_hash = current_hash;
                entry.contributions.clear();
                self.any_plugin_state_changed = true;
                continue;
            }

            // L3: contribution cache dirty flag intersection
            entry.contributions.retain(|region, _| {
                let deps = plugin.contribute_deps(region);
                !dirty.intersects(deps)
            });
        }
    }

    /// Initialize all plugins. Call after all plugins are registered.
    pub fn init_all(&mut self, state: &AppState) -> Vec<Command> {
        let mut commands = Vec::new();
        for plugin in &mut self.plugins {
            commands.extend(plugin.on_init(state));
        }
        commands
    }

    /// Shut down all plugins. Call before application exit.
    pub fn shutdown_all(&mut self) {
        for plugin in &mut self.plugins {
            plugin.on_shutdown();
        }
    }

    pub fn plugins_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn Plugin>> {
        self.plugins.iter_mut()
    }

    /// Collect surfaces from all plugins. Call after `init_all()`.
    pub fn collect_plugin_surfaces(&mut self) -> Vec<Box<dyn crate::surface::Surface>> {
        let mut surfaces = Vec::new();
        for plugin in &mut self.plugins {
            surfaces.extend(plugin.surfaces());
        }
        surfaces
    }

    /// Collect paint hooks from all plugins. Call after `init_all()`.
    pub fn collect_paint_hooks(&self) -> Vec<Box<dyn PaintHook>> {
        let mut hooks = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::PAINT_HOOK) {
                hooks.extend(plugin.paint_hooks());
            }
        }
        hooks
    }

    pub fn set_hit_map(&mut self, hit_map: HitMap) {
        self.hit_map = hit_map;
    }

    pub fn hit_test(&self, x: u16, y: u16) -> Option<InteractiveId> {
        self.hit_map.test(x, y)
    }

    /// Hit test returning both the InteractiveId and its bounding Rect.
    pub fn hit_test_with_rect(
        &self,
        x: u16,
        y: u16,
    ) -> Option<(InteractiveId, crate::layout::Rect)> {
        self.hit_map.test_with_rect(x, y)
    }

    // --- Menu item transformation ---

    /// Transform a menu item through all plugins. Returns None if no plugin transforms it.
    pub fn transform_menu_item(
        &self,
        item: &[crate::protocol::Atom],
        index: usize,
        selected: bool,
        state: &AppState,
    ) -> Option<Vec<crate::protocol::Atom>> {
        let mut current: Option<Vec<crate::protocol::Atom>> = None;
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::MENU_TRANSFORM) {
                continue;
            }
            let input = current.as_deref().unwrap_or(item);
            if let Some(transformed) = plugin.transform_menu_item(input, index, selected, state) {
                current = Some(transformed);
            }
        }
        current
    }

    // --- Cursor style override ---

    /// Query plugins for a cursor style override. Returns the first non-None.
    pub fn cursor_style_override(&self, state: &AppState) -> Option<crate::render::CursorStyle> {
        for (i, plugin) in self.plugins.iter().enumerate() {
            if !self.capabilities[i].contains(PluginCapabilities::CURSOR_STYLE) {
                continue;
            }
            if let Some(style) = plugin.cursor_style_override(state) {
                return Some(style);
            }
        }
        None
    }

    // ===========================================================================
    // New dispatch API: Contribute / Transform / Annotate
    // ===========================================================================

    /// Collect contributions from all plugins for a given region, sorted by priority.
    pub fn collect_contributions(
        &self,
        region: &SlotId,
        state: &AppState,
        ctx: &ContributeContext,
    ) -> Vec<Contribution> {
        let mut cache = self.slot_cache.borrow_mut();
        let mut contributions: Vec<Contribution> = self
            .plugins
            .iter()
            .enumerate()
            .filter_map(|(i, plugin)| {
                let caps = self.capabilities[i];
                if !caps.contains(PluginCapabilities::CONTRIBUTOR) {
                    return None;
                }
                // Check contribution cache
                if let Some(entry) = cache.entries.get(i)
                    && let Some(cached) = entry.contributions.get(region)
                {
                    return cached.clone();
                }
                let result = plugin.contribute_to(region, state, ctx);
                while cache.entries.len() <= i {
                    cache.entries.push(PluginCacheEntry::default());
                }
                cache.entries[i]
                    .contributions
                    .insert(region.clone(), result.clone());
                result
            })
            .collect();
        contributions.sort_by_key(|c| c.priority);
        contributions
    }

    /// Apply the transform chain for a given target.
    ///
    /// Plugins with the `TRANSFORMER` capability are collected into a chain,
    /// sorted by priority in **descending** order (high priority = inner =
    /// applied first). The `default_element_fn` is evaluated lazily as the
    /// seed element, then each transformer is applied in order.
    pub fn apply_transform_chain(
        &self,
        target: TransformTarget,
        default_element_fn: impl FnOnce() -> Element,
        state: &AppState,
    ) -> Element {
        let mut element = default_element_fn();

        // Collect (index, priority) for TRANSFORMER plugins
        let mut chain: Vec<(usize, i16)> = Vec::new();
        for (i, _plugin) in self.plugins.iter().enumerate() {
            if self.capabilities[i].contains(PluginCapabilities::TRANSFORMER) {
                let prio = self.plugins[i].transform_priority();
                chain.push((i, prio));
            }
        }
        // Sort by priority descending (high = inner = applied first)
        chain.sort_by_key(|&(_, prio)| std::cmp::Reverse(prio));

        for (pos, &(i, _)) in chain.iter().enumerate() {
            let ctx = TransformContext {
                is_default: true,
                chain_position: pos,
            };
            element = self.plugins[i].transform(&target, element, state, &ctx);
        }

        element
    }

    /// Collect annotations from all annotating plugins for visible lines.
    pub fn collect_annotations(&self, state: &AppState, ctx: &AnnotateContext) -> AnnotationResult {
        if !self.has_capability(PluginCapabilities::ANNOTATOR) {
            return AnnotationResult {
                left_gutter: None,
                right_gutter: None,
                line_backgrounds: None,
            };
        }

        let line_count = state.visible_line_range().len();
        let mut has_left = false;
        let mut has_right = false;
        let mut has_bg = false;

        let mut left_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut right_rows: Vec<FlexChild> = Vec::with_capacity(line_count);
        let mut backgrounds: Vec<Option<crate::protocol::Face>> = vec![None; line_count];

        for (line, bg_slot) in backgrounds.iter_mut().enumerate().take(line_count) {
            let mut left_parts: Vec<(i16, Element)> = Vec::new();
            let mut right_parts: Vec<(i16, Element)> = Vec::new();
            let mut bg_layers: Vec<BackgroundLayer> = Vec::new();

            for (i, plugin) in self.plugins.iter().enumerate() {
                if !self.capabilities[i].contains(PluginCapabilities::ANNOTATOR) {
                    continue;
                }
                if let Some(ann) = plugin.annotate_line_with_ctx(line, state, ctx) {
                    let prio = ann.priority;
                    if let Some(el) = ann.left_gutter {
                        left_parts.push((prio, el));
                        has_left = true;
                    }
                    if let Some(el) = ann.right_gutter {
                        right_parts.push((prio, el));
                        has_right = true;
                    }
                    if let Some(bg) = ann.background {
                        bg_layers.push(bg);
                    }
                }
            }

            // Sort gutter elements by priority (ascending: lower values first)
            left_parts.sort_by_key(|(prio, _)| *prio);
            right_parts.sort_by_key(|(prio, _)| *prio);

            let left_cell = match left_parts.len() {
                0 => Element::text(" ", crate::protocol::Face::default()),
                1 => left_parts.pop().unwrap().1,
                _ => Element::row(
                    left_parts
                        .into_iter()
                        .map(|(_, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            left_rows.push(FlexChild::fixed(left_cell));

            let right_cell = match right_parts.len() {
                0 => Element::text(" ", crate::protocol::Face::default()),
                1 => right_parts.pop().unwrap().1,
                _ => Element::row(
                    right_parts
                        .into_iter()
                        .map(|(_, el)| FlexChild::fixed(el))
                        .collect(),
                ),
            };
            right_rows.push(FlexChild::fixed(right_cell));

            if !bg_layers.is_empty() {
                bg_layers.sort_by_key(|l| l.z_order);
                *bg_slot = Some(bg_layers.last().unwrap().face);
                has_bg = true;
            }
        }

        AnnotationResult {
            left_gutter: if has_left {
                Some(Element::column(left_rows))
            } else {
                None
            },
            right_gutter: if has_right {
                Some(Element::column(right_rows))
            } else {
                None
            },
            line_backgrounds: if has_bg { Some(backgrounds) } else { None },
        }
    }

    /// Collect overlay contributions with collision-avoidance context.
    pub fn collect_overlays_with_ctx(
        &self,
        state: &AppState,
        ctx: &OverlayContext,
    ) -> Vec<OverlayContribution> {
        let mut contributions = Vec::new();
        for (i, plugin) in self.plugins.iter().enumerate() {
            let caps = self.capabilities[i];
            if (caps.contains(PluginCapabilities::CONTRIBUTOR)
                || caps.contains(PluginCapabilities::OVERLAY))
                && let Some(oc) = plugin.contribute_overlay_with_ctx(state, ctx)
            {
                contributions.push(oc);
            }
        }
        contributions.sort_by_key(|c| c.z_index);
        contributions
    }

    /// Check if any plugin has TRANSFORMER capability for a given target.
    pub fn has_transform_for(&self, _target: TransformTarget) -> bool {
        self.capabilities
            .iter()
            .any(|c| c.contains(PluginCapabilities::TRANSFORMER))
    }

    // --- Plugin message delivery ---

    /// Deliver a message to a specific plugin by ID.
    pub fn deliver_message(
        &mut self,
        target: &PluginId,
        payload: Box<dyn Any>,
        state: &AppState,
    ) -> (DirtyFlags, Vec<Command>) {
        for plugin in &mut self.plugins {
            if &plugin.id() == target {
                let mut commands = plugin.update(payload, state);
                let flags = extract_redraw_flags(&mut commands);
                return (flags, commands);
            }
        }
        (DirtyFlags::empty(), vec![])
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
