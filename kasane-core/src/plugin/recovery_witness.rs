//! Visual Faithfulness recovery witness (ADR-030 Level 4).
//!
//! A `RecoveryWitness` is evidence that a plugin's destructive display
//! directives (currently only `Hide`) are recoverable by a bounded user
//! interaction sequence, satisfying the Visual Faithfulness condition (§10.2a).

/// Recovery mechanism declared by a plugin for its destructive display directives.
#[derive(Debug, Clone)]
pub enum RecoveryMechanism {
    /// Recovery via a key binding toggle.
    KeyToggle { description: &'static str },
    /// Recovery via a plugin setting toggle.
    SettingToggle { key: &'static str },
    /// Recovery via another explicit mechanism.
    Declared { description: &'static str },
}

/// Evidence that a plugin's Hide directives are recoverable (§10.2a).
#[derive(Debug, Clone)]
pub struct RecoveryWitness {
    pub mechanism: RecoveryMechanism,
}
