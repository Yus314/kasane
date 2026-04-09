use super::{
    DEFAULT_PLUGIN_DIAGNOSTIC_OVERLAY_LINES, ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES,
    PLUGIN_ACTIVATION_OVERLAY_TITLE, PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
    PLUGIN_DISCOVERY_OVERLAY_TITLE, PLUGIN_ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES, PluginDiagnostic,
    PluginDiagnosticKind, PluginDiagnosticOverlayLine, PluginDiagnosticOverlayTagKind,
    PluginDiagnosticSeverity, PluginDiagnosticTarget, ProviderArtifactStage,
    summarize_plugin_diagnostic,
};

// Backdrop tone scoring -- used to decide whether the overlay is colored as
// "activation" (plugin errors dominate), "discovery" (provider errors dominate),
// or "neutral" (mixed). Errors weigh 3x more than warnings so a single error
// outweighs a warning even when the warning has a tag bonus.
const OVERLAY_WARNING_SCORE: u32 = 2;
const OVERLAY_ERROR_SCORE: u32 = 6;
/// Minimum score gap to declare one target category dominant for backdrop tone.
const OVERLAY_SCORE_DOMINANCE_DELTA: u32 = 3;
/// Score gap threshold for strong dominance, used in mixed-batch provider
/// artifact stage quota allocation (allows up to 3 provider lines).
const OVERLAY_SCORE_STRONG_DOMINANCE_DELTA: u32 = 6;

#[derive(Clone, Debug)]
pub(super) struct OverlayBucket {
    pub(super) line: PluginDiagnosticOverlayLine,
    pub(super) priority: (u8, u8, u8, u8),
    pub(super) target: PluginDiagnosticTarget,
    pub(super) last_seen: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverlayBackdropTone {
    Neutral,
    Activation,
    Discovery,
}

pub(super) fn overlay_title(buckets: &[OverlayBucket]) -> &'static str {
    let has_plugin = buckets
        .iter()
        .any(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Plugin(_)));
    let has_provider = buckets
        .iter()
        .any(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Provider(_)));

    match (has_plugin, has_provider) {
        (true, true) => PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
        (true, false) => PLUGIN_ACTIVATION_OVERLAY_TITLE,
        (false, true) => PLUGIN_DISCOVERY_OVERLAY_TITLE,
        (false, false) => PLUGIN_DIAGNOSTIC_OVERLAY_TITLE,
    }
}

pub(super) fn overlay_backdrop_tone_for_title(title: &str) -> OverlayBackdropTone {
    match title {
        PLUGIN_ACTIVATION_OVERLAY_TITLE => OverlayBackdropTone::Activation,
        PLUGIN_DISCOVERY_OVERLAY_TITLE => OverlayBackdropTone::Discovery,
        _ => OverlayBackdropTone::Neutral,
    }
}

/// Choose the backdrop tone by comparing weighted scores of plugin vs provider
/// diagnostics. The dominant category colors the overlay; if neither dominates
/// by at least `OVERLAY_SCORE_DOMINANCE_DELTA`, the tone is neutral.
pub(super) fn overlay_backdrop_tone(buckets: &[OverlayBucket]) -> OverlayBackdropTone {
    let plugin_score = overlay_target_score(buckets, true);
    let provider_score = overlay_target_score(buckets, false);

    match (plugin_score, provider_score) {
        (0, 0) => OverlayBackdropTone::Neutral,
        (0, _) => OverlayBackdropTone::Discovery,
        (_, 0) => OverlayBackdropTone::Activation,
        (plugin, provider) if plugin >= provider.saturating_add(OVERLAY_SCORE_DOMINANCE_DELTA) => {
            OverlayBackdropTone::Activation
        }
        (plugin, provider) if provider >= plugin.saturating_add(OVERLAY_SCORE_DOMINANCE_DELTA) => {
            OverlayBackdropTone::Discovery
        }
        _ => OverlayBackdropTone::Neutral,
    }
}

fn overlay_target_score(buckets: &[OverlayBucket], plugin_target: bool) -> u32 {
    buckets
        .iter()
        .filter(|bucket| {
            if plugin_target {
                matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
            } else {
                matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
            }
        })
        .map(|bucket| overlay_line_score(&bucket.line) * bucket.line.repeat_count as u32)
        .sum()
}

fn overlay_line_score(line: &PluginDiagnosticOverlayLine) -> u32 {
    overlay_severity_weight(line.severity) + overlay_tag_score_bonus(line.tag_kind)
}

fn overlay_severity_weight(severity: PluginDiagnosticSeverity) -> u32 {
    match severity {
        PluginDiagnosticSeverity::Warning => OVERLAY_WARNING_SCORE,
        PluginDiagnosticSeverity::Error => OVERLAY_ERROR_SCORE,
    }
}

fn overlay_tag_score_bonus(kind: PluginDiagnosticOverlayTagKind) -> u32 {
    match kind {
        PluginDiagnosticOverlayTagKind::ArtifactManifest => 0,
        PluginDiagnosticOverlayTagKind::ArtifactRead => 0,
        PluginDiagnosticOverlayTagKind::ArtifactLoad => 1,
        PluginDiagnosticOverlayTagKind::ArtifactInstantiate => 2,
        PluginDiagnosticOverlayTagKind::Activation
        | PluginDiagnosticOverlayTagKind::Discovery
        | PluginDiagnosticOverlayTagKind::Runtime
        | PluginDiagnosticOverlayTagKind::Config => 0,
    }
}

pub fn diagnostic_overlay_lines(
    diagnostics: &[PluginDiagnostic],
    max_lines: usize,
) -> Vec<PluginDiagnosticOverlayLine> {
    select_visible_overlay_buckets(&collect_diagnostic_overlay_buckets(diagnostics), max_lines)
        .into_iter()
        .map(|bucket| bucket.line)
        .collect()
}

pub(super) fn collect_diagnostic_overlay_buckets(
    diagnostics: &[PluginDiagnostic],
) -> Vec<OverlayBucket> {
    let mut buckets: Vec<OverlayBucket> = Vec::new();

    for (index, diagnostic) in diagnostics.iter().enumerate() {
        let severity = diagnostic.severity();
        let text = summarize_plugin_diagnostic(diagnostic);
        let priority = diagnostic_overlay_priority(diagnostic);
        let tag_kind = diagnostic_overlay_tag_kind(diagnostic);
        if let Some(existing) = buckets.iter_mut().find(|bucket| {
            bucket.line.severity == severity
                && bucket.line.tag_kind == tag_kind
                && bucket.line.text == text
        }) {
            existing.line.repeat_count += 1;
            existing.last_seen = index;
            existing.priority = priority;
            continue;
        }

        buckets.push(OverlayBucket {
            line: PluginDiagnosticOverlayLine {
                severity,
                tag_kind,
                text,
                repeat_count: 1,
            },
            priority,
            target: diagnostic.target.clone(),
            last_seen: index,
        });
    }

    sort_overlay_buckets(&mut buckets);
    buckets
}

pub(super) fn merge_overlay_buckets(
    previous: Vec<OverlayBucket>,
    current: Vec<OverlayBucket>,
) -> Vec<OverlayBucket> {
    if previous.is_empty() {
        return current;
    }
    if current.is_empty() {
        return previous;
    }

    let previous_len = previous.len();
    let mut buckets: Vec<OverlayBucket> = previous
        .into_iter()
        .enumerate()
        .map(|(index, mut bucket)| {
            bucket.last_seen = index;
            bucket
        })
        .collect();

    for (index, bucket) in current.into_iter().enumerate() {
        if let Some(existing) = buckets.iter_mut().find(|existing| {
            existing.line.severity == bucket.line.severity
                && existing.line.tag_kind == bucket.line.tag_kind
                && existing.line.text == bucket.line.text
        }) {
            existing.line.repeat_count += bucket.line.repeat_count;
            existing.priority = bucket.priority;
            existing.last_seen = previous_len + index;
            continue;
        }
        buckets.push(OverlayBucket {
            last_seen: previous_len + index,
            ..bucket
        });
    }

    sort_overlay_buckets(&mut buckets);
    buckets
}

pub(super) fn overlay_line_limit(buckets: &[OverlayBucket]) -> usize {
    if buckets.iter().any(|bucket| {
        bucket.line.severity == PluginDiagnosticSeverity::Error
            && matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
    }) {
        PLUGIN_ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    } else if buckets
        .iter()
        .any(|bucket| bucket.line.severity == PluginDiagnosticSeverity::Error)
    {
        ERROR_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    } else {
        DEFAULT_PLUGIN_DIAGNOSTIC_OVERLAY_LINES
    }
}

/// Select which diagnostic buckets to display in the overlay, respecting `limit`.
///
/// For mixed overlays (both plugin and provider diagnostics), uses a multi-pass
/// quota reservation strategy: plugin errors first, then provider errors, then
/// warnings, then remaining slots filled by priority order.
pub(super) fn select_visible_overlay_buckets(
    buckets: &[OverlayBucket],
    limit: usize,
) -> Vec<OverlayBucket> {
    if limit == 0 || buckets.is_empty() {
        return vec![];
    }

    let has_plugin = buckets
        .iter()
        .any(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Plugin(_)));
    let has_provider = buckets
        .iter()
        .any(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Provider(_)));
    if !has_plugin && has_provider {
        return select_provider_overlay_buckets(buckets, limit);
    }
    if !has_plugin || !has_provider {
        return buckets.iter().take(limit).cloned().collect();
    }

    let mut selected = Vec::new();
    let mut marked = vec![false; buckets.len()];

    reserve_overlay_indexes(
        &mut selected,
        &mut marked,
        buckets,
        limit,
        mixed_overlay_plugin_error_quota(buckets, limit),
        |bucket| {
            matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
                && bucket.line.severity == PluginDiagnosticSeverity::Error
        },
    );
    let remaining_after_plugin_errors = limit.saturating_sub(selected.len());
    reserve_overlay_indexes(
        &mut selected,
        &mut marked,
        buckets,
        limit,
        mixed_overlay_provider_error_quota(buckets, remaining_after_plugin_errors),
        provider_discovery_error,
    );
    if buckets
        .iter()
        .any(|bucket| bucket.line.severity == PluginDiagnosticSeverity::Warning)
    {
        reserve_overlay_indexes(&mut selected, &mut marked, buckets, limit, 1, |bucket| {
            matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
                && bucket.line.severity == PluginDiagnosticSeverity::Warning
        });
        let remaining_after_plugin_warning = limit.saturating_sub(selected.len());
        reserve_provider_artifact_stage_indexes(
            &mut selected,
            &mut marked,
            buckets,
            limit,
            mixed_overlay_provider_artifact_stage_quota(buckets, remaining_after_plugin_warning),
        );
    }
    reserve_overlay_indexes(
        &mut selected,
        &mut marked,
        buckets,
        limit,
        mixed_overlay_plugin_quota(buckets, limit),
        |bucket| matches!(bucket.target, PluginDiagnosticTarget::Plugin(_)),
    );
    let remaining_after_plugin_target = limit.saturating_sub(selected.len());
    reserve_overlay_indexes(
        &mut selected,
        &mut marked,
        buckets,
        limit,
        mixed_overlay_provider_quota(buckets, remaining_after_plugin_target),
        |bucket| matches!(bucket.target, PluginDiagnosticTarget::Provider(_)),
    );

    for (idx, is_marked) in marked.iter_mut().enumerate() {
        if selected.len() >= limit {
            break;
        }
        if !*is_marked {
            selected.push(idx);
            *is_marked = true;
        }
    }

    selected.sort_unstable();
    selected
        .into_iter()
        .take(limit)
        .map(|idx| buckets[idx].clone())
        .collect()
}

fn select_provider_overlay_buckets(buckets: &[OverlayBucket], limit: usize) -> Vec<OverlayBucket> {
    if limit == 0 || buckets.is_empty() {
        return vec![];
    }

    let has_provider_error = buckets.iter().any(provider_discovery_error);
    let has_provider_warning = buckets.iter().any(provider_artifact_warning);
    if !has_provider_error && !has_provider_warning {
        return buckets.iter().take(limit).cloned().collect();
    }

    let mut selected = Vec::new();
    let mut marked = vec![false; buckets.len()];
    if has_provider_error {
        reserve_overlay_indexes(
            &mut selected,
            &mut marked,
            buckets,
            limit,
            1,
            provider_discovery_error,
        );
    }
    if has_provider_warning {
        let remaining_limit = limit.saturating_sub(selected.len());
        reserve_provider_artifact_stage_indexes(
            &mut selected,
            &mut marked,
            buckets,
            limit,
            provider_artifact_stage_quota(buckets, remaining_limit),
        );
    }
    for (idx, is_marked) in marked.iter_mut().enumerate() {
        if selected.len() >= limit {
            break;
        }
        if !*is_marked {
            selected.push(idx);
            *is_marked = true;
        }
    }

    selected.sort_unstable();
    selected
        .into_iter()
        .take(limit)
        .map(|idx| buckets[idx].clone())
        .collect()
}

fn provider_discovery_error(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Error
        && bucket.line.tag_kind == PluginDiagnosticOverlayTagKind::Discovery
}

fn provider_artifact_warning(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Warning
        && matches!(
            bucket.line.tag_kind,
            PluginDiagnosticOverlayTagKind::ArtifactManifest
                | PluginDiagnosticOverlayTagKind::ArtifactRead
                | PluginDiagnosticOverlayTagKind::ArtifactLoad
                | PluginDiagnosticOverlayTagKind::ArtifactInstantiate
        )
}

fn provider_artifact_stage_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    [
        provider_artifact_instantiate_warning as fn(&OverlayBucket) -> bool,
        provider_artifact_load_warning,
        provider_artifact_read_warning,
        provider_artifact_manifest_warning,
    ]
    .into_iter()
    .filter(|predicate| buckets.iter().any(*predicate))
    .count()
    .min(limit)
}

fn provider_artifact_instantiate_warning(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Warning
        && bucket.line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactInstantiate
}

fn provider_artifact_load_warning(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Warning
        && bucket.line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactLoad
}

fn provider_artifact_read_warning(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Warning
        && bucket.line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactRead
}

fn provider_artifact_manifest_warning(bucket: &OverlayBucket) -> bool {
    matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
        && bucket.line.severity == PluginDiagnosticSeverity::Warning
        && bucket.line.tag_kind == PluginDiagnosticOverlayTagKind::ArtifactManifest
}

fn reserve_provider_artifact_stage_indexes(
    selected: &mut Vec<usize>,
    marked: &mut [bool],
    buckets: &[OverlayBucket],
    limit: usize,
    quota: usize,
) {
    if quota == 0 || selected.len() >= limit {
        return;
    }

    let predicates: [fn(&OverlayBucket) -> bool; 4] = [
        provider_artifact_instantiate_warning,
        provider_artifact_load_warning,
        provider_artifact_read_warning,
        provider_artifact_manifest_warning,
    ];
    for predicate in predicates {
        reserve_overlay_indexes(selected, marked, buckets, limit, 1, predicate);
        if selected.len() >= limit
            || quota
                <= selected
                    .iter()
                    .filter(|&&i| provider_artifact_warning(&buckets[i]))
                    .count()
        {
            break;
        }
    }
}

fn reserve_overlay_indexes<F>(
    selected: &mut Vec<usize>,
    marked: &mut [bool],
    buckets: &[OverlayBucket],
    limit: usize,
    quota: usize,
    predicate: F,
) where
    F: Fn(&OverlayBucket) -> bool,
{
    if quota == 0 || selected.len() >= limit {
        return;
    }

    for (idx, bucket) in buckets.iter().enumerate() {
        if selected.len() >= limit
            || quota <= selected.iter().filter(|&&i| predicate(&buckets[i])).count()
        {
            break;
        }
        if marked[idx] || !predicate(bucket) {
            continue;
        }
        selected.push(idx);
        marked[idx] = true;
    }
}

/// Base quota for plugin-targeted lines in a mixed overlay.
/// Reserves up to 2 slots if plugin errors exist, otherwise 1.
fn mixed_overlay_plugin_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    let plugin_count = buckets
        .iter()
        .filter(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Plugin(_)))
        .count();
    if plugin_count == 0 {
        return 0;
    }

    let desired = if buckets.iter().any(|bucket| {
        matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
            && bucket.line.severity == PluginDiagnosticSeverity::Error
    }) {
        2
    } else {
        1
    };

    desired.min(plugin_count).min(limit)
}

/// At most 2 plugin error lines in a mixed overlay, to leave room for provider lines.
fn mixed_overlay_plugin_error_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    let error_count = buckets
        .iter()
        .filter(|bucket| {
            matches!(bucket.target, PluginDiagnosticTarget::Plugin(_))
                && bucket.line.severity == PluginDiagnosticSeverity::Error
        })
        .count();
    error_count.min(2).min(limit)
}

/// At most 1 general provider line in a mixed overlay.
fn mixed_overlay_provider_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    let provider_count = buckets
        .iter()
        .filter(|bucket| matches!(bucket.target, PluginDiagnosticTarget::Provider(_)))
        .count();
    provider_count.min(usize::from(limit > 0))
}

/// At most 1 provider error line (discovery failure) in a mixed overlay.
fn mixed_overlay_provider_error_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    let error_count = buckets
        .iter()
        .filter(|bucket| {
            matches!(bucket.target, PluginDiagnosticTarget::Provider(_))
                && bucket.line.severity == PluginDiagnosticSeverity::Error
        })
        .count();
    error_count.min(usize::from(limit > 0))
}

/// Provider artifact stage lines in a mixed overlay, scaled by score dominance:
/// strong dominance -> up to 3, normal dominance -> up to 2, otherwise -> up to 1.
fn mixed_overlay_provider_artifact_stage_quota(buckets: &[OverlayBucket], limit: usize) -> usize {
    let stage_count = provider_artifact_stage_quota(buckets, limit);
    if stage_count == 0 {
        return 0;
    }

    let plugin_score = overlay_target_score(buckets, true);
    let provider_score = overlay_target_score(buckets, false);

    if provider_score >= plugin_score.saturating_add(OVERLAY_SCORE_STRONG_DOMINANCE_DELTA) {
        stage_count.min(3)
    } else if provider_score >= plugin_score.saturating_add(OVERLAY_SCORE_DOMINANCE_DELTA) {
        stage_count.min(2)
    } else {
        stage_count.min(1)
    }
}

fn sort_overlay_buckets(buckets: &mut [OverlayBucket]) {
    buckets.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| right.last_seen.cmp(&left.last_seen))
    });
}

fn diagnostic_overlay_priority(diagnostic: &PluginDiagnostic) -> (u8, u8, u8, u8) {
    let severity = match diagnostic.severity() {
        PluginDiagnosticSeverity::Error => 2,
        PluginDiagnosticSeverity::Warning => 1,
    };
    let target = match diagnostic.target {
        PluginDiagnosticTarget::Plugin(_) => 2,
        PluginDiagnosticTarget::Provider(_) => 1,
    };
    let kind = match diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. } => 3,
        PluginDiagnosticKind::InstantiationFailed => 2,
        PluginDiagnosticKind::RuntimeError { .. } => 1,
        PluginDiagnosticKind::ProviderCollectFailed => 1,
        PluginDiagnosticKind::ConfigError { .. } => 1,
        PluginDiagnosticKind::ProviderArtifactFailed { .. } => 0,
    };
    let stage = match diagnostic.kind {
        PluginDiagnosticKind::ProviderArtifactFailed { stage, .. } => {
            provider_artifact_overlay_priority(stage)
        }
        _ => 0,
    };
    (severity, target, kind, stage)
}

fn provider_artifact_overlay_priority(stage: ProviderArtifactStage) -> u8 {
    match stage {
        ProviderArtifactStage::Instantiate => 3,
        ProviderArtifactStage::Load => 2,
        ProviderArtifactStage::Read => 1,
        ProviderArtifactStage::Manifest => 0,
    }
}

fn diagnostic_overlay_tag_kind(diagnostic: &PluginDiagnostic) -> PluginDiagnosticOverlayTagKind {
    match diagnostic.kind {
        PluginDiagnosticKind::SurfaceRegistrationFailed { .. }
        | PluginDiagnosticKind::InstantiationFailed => PluginDiagnosticOverlayTagKind::Activation,
        PluginDiagnosticKind::ProviderCollectFailed => PluginDiagnosticOverlayTagKind::Discovery,
        PluginDiagnosticKind::ProviderArtifactFailed { stage, .. } => match stage {
            ProviderArtifactStage::Manifest => PluginDiagnosticOverlayTagKind::ArtifactManifest,
            ProviderArtifactStage::Read => PluginDiagnosticOverlayTagKind::ArtifactRead,
            ProviderArtifactStage::Load => PluginDiagnosticOverlayTagKind::ArtifactLoad,
            ProviderArtifactStage::Instantiate => {
                PluginDiagnosticOverlayTagKind::ArtifactInstantiate
            }
        },
        PluginDiagnosticKind::RuntimeError { .. } => PluginDiagnosticOverlayTagKind::Runtime,
        PluginDiagnosticKind::ConfigError { .. } => PluginDiagnosticOverlayTagKind::Config,
    }
}
