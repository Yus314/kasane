# ADR-011: CLI Design — kak Drop-in Replacement

**Status:** Decided

**Context:**
kasane is a Kakoune UI frontend, not "a different editor." The goal is to minimize friction when kak users migrate to kasane, achieving a state where `alias kak=kasane` works completely.

**Decision:** Design kasane as a drop-in replacement for kak. Adopt the following 10 items.

### 11-1: Basic Policy — Drop-in Replacement

**Decision:** Guarantee that when kak is replaced with kasane via `alias kak=kasane` or PATH manipulation, all kak workflows work correctly.

**Rationale:**
- kasane is "a different UI" for Kakoune; users should perceive they are "using Kakoune"
- Same pattern as Neovide (GUI frontend for nvim): launched by frontend name, passing arguments to the backend
- When `$EDITOR=kasane` is set, kasane UI is used in git commit, ranger, and everything else

### 11-2: Non-UI Operation Delegation — exec

**Decision:** When non-UI operations (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) are detected, replace the kasane process with kak via `exec`. `-ui json` is not appended.

**Rationale:**
- exec completely replaces the kasane process with kak, resulting in zero overhead
- The most Unix-correct approach (no unnecessary parent process left behind)
- Resolves the current inaccuracy of appending `-ui json` to non-UI operations

**Non-UI flag detection:** Hardcoded explicit list (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`). Manually added when kak adds new flags.

### 11-3: Flag System — Pre/Post `--` Separation

**Decision:** kasane-specific flags use GNU-convention `--long-option` format. kak flags are passed through as-is. `--` provides explicit separation.

**kasane-specific flags:**
- `--ui {tui|gui}` — Backend selection (one-shot override)
- `--version` — Display both kasane and kak versions
- `--help` — Display kasane help

**Parsing rules:**
1. Before `--`: Extract kasane-specific flags (`--ui`, `--version`, `--help`). Everything else is accumulated as kak arguments
2. After `--`: Everything is accumulated as kak arguments
3. Error rejection if kasane-specific flags and non-UI flags are mixed

**Rationale:**
- Clear separation: `--` (double dash) for kasane, `-` (single dash) for kak
- Avoids collision with kak's `-ui` (`kasane -ui gui` passes `-ui` and `gui` to kak)
- Safe for future flag additions (`--config`, `--log-level`, etc.)

### 11-4: Session Name Interception — Both `-c` and `-s`

**Decision:** Intercept both `-c` (session connect) and `-s` (session create) to have kasane retain the session name. Arguments are also passed through to kak.

**Rationale:**
- Display session name in GUI window title (`kasane — project`)
- Log with `[session=project]`
- Future session-specific config (`~/.config/kasane/sessions/project.toml`) extension
- Extremely small additional cost (a few lines of change)

### 11-5: Default UI Mode — Configurable via kasane.kdl

**Decision:** Make the default UI mode (TUI/GUI) configurable via `ui { backend }` in `kasane.kdl`. The `--ui` flag serves as a one-shot override.

**Rationale:**
- Users who want GUI as default no longer need to include `--ui gui` in their alias
- Practically eliminates the mixed kasane-specific/non-UI flag error
- Full migration possible with just `alias kak=kasane`

### 11-6: `--version` Output — Both kasane + kak

**Decision:** `kasane --version` displays both kasane and kak versions.

```
kasane 0.1.0 (kakoune vXXXX.XX.XX)
```

**Rationale:**
- Useful to know both versions when debugging
- `kasane -version` is exec-delegated to kak, displaying only kak's version (clear distinction)

### 11-7: Mixed Flag Behavior — Error Rejection

**Decision:** When kasane-specific flags (`--ui`, `--version`, `--help`) and non-UI flags (`-l`, `-f`, `-p`, `-d`, `-clear`, `-version`, `-help`) are specified simultaneously, reject with an error.

```
kasane --ui gui -l
→ error: --ui cannot be combined with -l (non-UI operation)
```

**Rationale:**
- Backend selection is meaningless for non-UI operations; early detection of user mistakes
- Making the default UI configurable via `kasane.kdl` removes the motivation to include `--ui` in aliases, so this error practically never occurs
- Explicit errors over silent ignoring follows Rust ecosystem conventions

### 11-8: Native kak UI Fallback — Not Provided

**Decision:** No means is provided to fall back to the native kak terminal UI via kasane.

**Rationale:**
- Users who want the native UI can run kak directly
- kasane's raison d'être is "providing a different UI," and a fallback to the native UI would be contradictory

### Processing Flow

```
parse_cli_args(args)
├── 1. Extract kasane-specific flags (--ui, --version, --help)
├── 2. Extract interception targets (-c, -s → retain session name + also pass to kak)
├── 3. Detect non-UI flags (-l, -f, -p, -d, -clear, -version, -help)
├── 4. Mixed check (kasane-specific ∩ non-UI ≠ ∅ → error)
└── Result:
    ├── CliAction::KasaneVersion        ← --version
    ├── CliAction::KasaneHelp           ← --help
    ├── CliAction::DelegateToKak(args)  ← non-UI flag detected → exec kak
    └── CliAction::RunKasane { session, ui_mode, kak_args }  ← UI startup
```

### Examples

```bash
# Basic usage (drop-in)
kasane file.txt                    # → kak -ui json file.txt
kasane -c project                  # → kak -ui json -c project (session name retained)
kasane -s myses file.txt           # → kak -ui json -s myses file.txt (session name retained)
kasane -e "buffer-next"            # → kak -ui json -e "buffer-next"
kasane -n -ro file.txt             # → kak -ui json -n -ro file.txt

# kasane-specific flags
kasane --ui gui file.txt           # → Launch with GUI backend
kasane --version                   # → "kasane 0.1.0 (kakoune vXXXX.XX.XX)"
kasane --help                      # → Display kasane help

# Non-UI operations (delegated to kak via exec)
kasane -l                          # → exec kak -l
kasane -f "gg"                     # → exec kak -f "gg"
kasane -p session                  # → exec kak -p session
kasane -d -s daemon                # → exec kak -d -s daemon
kasane -version                    # → exec kak -version
kasane -help                       # → exec kak -help

# Error case
kasane --ui gui -l                 # → Error: --ui cannot be combined with -l

# Explicit separation with --
kasane --ui gui -- -e "echo hello" # → kak -ui json -e "echo hello" (GUI launch)
```
