pub const NON_UI_FLAGS: &[&str] = &["-l", "-f", "-p", "-d", "-clear", "-version", "-help"];
pub const NON_UI_FLAGS_WITH_ARG: &[&str] = &["-f", "-p"];
pub const KAK_FLAGS_WITH_ARG: &[&str] = &["-e", "-E", "-i", "-debug"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Tui,
    Gui,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    ShowVersion,
    ShowHelp,
    DelegateToKak(Vec<String>),
    RunKasane {
        session: Option<String>,
        ui_mode: Option<UiMode>,
        kak_args: Vec<String>,
    },
    Plugin(PluginSubcommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginSubcommand {
    New {
        name: String,
        template: PluginTemplate,
    },
    Build {
        path: Option<String>,
    },
    Install {
        path: Option<String>,
    },
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PluginTemplate {
    Annotation,
    #[default]
    Contribution,
    Transform,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    UnknownUiMode(String),
    MissingUiArg,
    ConflictingFlags { kasane_flag: &'static str },
    PluginMissingSubcommand,
    PluginUnknownSubcommand(String),
    PluginMissingName,
    PluginUnknownTemplate(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::UnknownUiMode(mode) => {
                write!(f, "unknown --ui mode: {mode}. Use 'tui' or 'gui'.")
            }
            CliError::MissingUiArg => write!(f, "--ui requires an argument (tui or gui)"),
            CliError::ConflictingFlags { kasane_flag } => {
                write!(
                    f,
                    "cannot combine kasane flag {kasane_flag} with non-UI kak flags"
                )
            }
            CliError::PluginMissingSubcommand => {
                write!(
                    f,
                    "missing subcommand. Usage: kasane plugin <new|build|install|list>"
                )
            }
            CliError::PluginUnknownSubcommand(s) => {
                write!(
                    f,
                    "unknown plugin subcommand: {s}. Use new, build, install, or list."
                )
            }
            CliError::PluginMissingName => {
                write!(f, "missing plugin name. Usage: kasane plugin new <name>")
            }
            CliError::PluginUnknownTemplate(t) => {
                write!(
                    f,
                    "unknown template: {t}. Use contribution, annotation, or transform."
                )
            }
        }
    }
}

pub fn parse_cli_args(args: &[String]) -> Result<CliAction, CliError> {
    let mut session = None;
    let mut ui_mode = None;
    let mut kak_args = Vec::new();
    let mut iter = args.iter();
    let mut pass_through = false;
    let mut has_kasane_flags = false;
    let mut has_non_ui_flags = false;
    let mut kasane_flag_name: &'static str = "";

    while let Some(arg) = iter.next() {
        if pass_through {
            kak_args.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "--version" => {
                has_kasane_flags = true;
                kasane_flag_name = "--version";
            }
            "--help" => {
                has_kasane_flags = true;
                kasane_flag_name = "--help";
            }
            "--ui" => {
                has_kasane_flags = true;
                kasane_flag_name = "--ui";
                match iter.next() {
                    Some(mode) => match mode.as_str() {
                        "tui" => ui_mode = Some(UiMode::Tui),
                        "gui" => ui_mode = Some(UiMode::Gui),
                        _ => return Err(CliError::UnknownUiMode(mode.clone())),
                    },
                    None => return Err(CliError::MissingUiArg),
                }
            }
            "plugin" if kak_args.is_empty() && !has_kasane_flags && !has_non_ui_flags => {
                let subcmd = parse_plugin_args(&mut iter)?;
                return Ok(CliAction::Plugin(subcmd));
            }
            "--" => {
                pass_through = true;
            }
            "-c" | "-s" => {
                kak_args.push(arg.clone());
                if let Some(s) = iter.next() {
                    session = Some(s.clone());
                    kak_args.push(s.clone());
                }
            }
            flag if KAK_FLAGS_WITH_ARG.contains(&flag) => {
                kak_args.push(arg.clone());
                if let Some(next) = iter.next() {
                    kak_args.push(next.clone());
                }
            }
            flag if NON_UI_FLAGS.contains(&flag) => {
                has_non_ui_flags = true;
                kak_args.push(arg.clone());
                if NON_UI_FLAGS_WITH_ARG.contains(&flag)
                    && let Some(next) = iter.next()
                {
                    kak_args.push(next.clone());
                }
            }
            _ => {
                kak_args.push(arg.clone());
            }
        }
    }

    if has_kasane_flags && has_non_ui_flags {
        return Err(CliError::ConflictingFlags {
            kasane_flag: kasane_flag_name,
        });
    }

    if has_non_ui_flags {
        return Ok(CliAction::DelegateToKak(kak_args));
    }

    if has_kasane_flags && ui_mode.is_none() && session.is_none() && kak_args.is_empty() {
        if kasane_flag_name == "--version" {
            return Ok(CliAction::ShowVersion);
        }
        if kasane_flag_name == "--help" {
            return Ok(CliAction::ShowHelp);
        }
    }

    Ok(CliAction::RunKasane {
        session,
        ui_mode,
        kak_args,
    })
}

fn parse_plugin_args<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
) -> Result<PluginSubcommand, CliError> {
    let sub = iter.next().ok_or(CliError::PluginMissingSubcommand)?;
    match sub.as_str() {
        "new" => {
            let name = iter.next().ok_or(CliError::PluginMissingName)?.clone();
            let mut template = PluginTemplate::default();
            if let Some(flag) = iter.next()
                && flag == "--template"
            {
                let t = iter
                    .next()
                    .ok_or(CliError::PluginUnknownTemplate(String::new()))?;
                template = match t.as_str() {
                    "annotation" | "annotate" => PluginTemplate::Annotation,
                    "contribution" | "contribute" => PluginTemplate::Contribution,
                    "transform" => PluginTemplate::Transform,
                    _ => return Err(CliError::PluginUnknownTemplate(t.clone())),
                };
            }
            Ok(PluginSubcommand::New { name, template })
        }
        "build" => Ok(PluginSubcommand::Build {
            path: iter.next().cloned(),
        }),
        "install" => Ok(PluginSubcommand::Install {
            path: iter.next().cloned(),
        }),
        "list" => Ok(PluginSubcommand::List),
        other => Err(CliError::PluginUnknownSubcommand(other.to_string())),
    }
}

pub fn print_help() {
    println!(
        "\
kasane {} - Kakoune frontend

Usage: kasane [kasane-options] [kak-options] [file]... [+<line>[:<col>]|+:]

Kasane options:
  --ui <tui|gui>   Select UI backend (default: config.toml [ui] backend)
  --version        Show kasane and kak versions
  --help           Show this help message

Subcommands:
  plugin new <name> [--template T]  Create a new plugin project (T: contribution, annotation, transform)
  plugin build [<path>]             Build plugin for wasm32-wasip2
  plugin install [<path>]           Build, validate, and install plugin
  plugin list                       Show installed plugins

All other options are passed to kak. Non-UI kak flags (-l, -f, -p, -d,
-clear, -version, -help) are delegated directly to kak.

Examples:
  kasane file.txt              Edit file with default UI
  kasane --ui gui file.txt     Edit file with GUI backend
  kasane -c project            Connect to session 'project'
  kasane -l                    List kak sessions (delegates to kak)",
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn test_no_args() {
        assert_eq!(
            parse_cli_args(&args(&[])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: vec![],
            })
        );
    }

    #[test]
    fn test_file() {
        assert_eq!(
            parse_cli_args(&args(&["file.txt"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: args(&["file.txt"]),
            })
        );
    }

    #[test]
    fn test_connect_session() {
        assert_eq!(
            parse_cli_args(&args(&["-c", "project"])),
            Ok(CliAction::RunKasane {
                session: Some("project".to_string()),
                ui_mode: None,
                kak_args: args(&["-c", "project"]),
            })
        );
    }

    #[test]
    fn test_named_session() {
        assert_eq!(
            parse_cli_args(&args(&["-s", "myses", "file.txt"])),
            Ok(CliAction::RunKasane {
                session: Some("myses".to_string()),
                ui_mode: None,
                kak_args: args(&["-s", "myses", "file.txt"]),
            })
        );
    }

    #[test]
    fn test_ui_gui() {
        assert_eq!(
            parse_cli_args(&args(&["--ui", "gui", "file.txt"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: Some(UiMode::Gui),
                kak_args: args(&["file.txt"]),
            })
        );
    }

    #[test]
    fn test_version() {
        assert_eq!(
            parse_cli_args(&args(&["--version"])),
            Ok(CliAction::ShowVersion)
        );
    }

    #[test]
    fn test_help() {
        assert_eq!(parse_cli_args(&args(&["--help"])), Ok(CliAction::ShowHelp));
    }

    #[test]
    fn test_delegate_list() {
        assert_eq!(
            parse_cli_args(&args(&["-l"])),
            Ok(CliAction::DelegateToKak(args(&["-l"])))
        );
    }

    #[test]
    fn test_delegate_filter() {
        assert_eq!(
            parse_cli_args(&args(&["-f", "gg", "file.txt"])),
            Ok(CliAction::DelegateToKak(args(&["-f", "gg", "file.txt"])))
        );
    }

    #[test]
    fn test_delegate_pipe() {
        assert_eq!(
            parse_cli_args(&args(&["-p", "session"])),
            Ok(CliAction::DelegateToKak(args(&["-p", "session"])))
        );
    }

    #[test]
    fn test_delegate_daemon() {
        assert_eq!(
            parse_cli_args(&args(&["-d", "-s", "daemon"])),
            Ok(CliAction::DelegateToKak(args(&["-d", "-s", "daemon"])))
        );
    }

    #[test]
    fn test_delegate_clear() {
        assert_eq!(
            parse_cli_args(&args(&["-clear"])),
            Ok(CliAction::DelegateToKak(args(&["-clear"])))
        );
    }

    #[test]
    fn test_delegate_kak_version() {
        assert_eq!(
            parse_cli_args(&args(&["-version"])),
            Ok(CliAction::DelegateToKak(args(&["-version"])))
        );
    }

    #[test]
    fn test_delegate_kak_help() {
        assert_eq!(
            parse_cli_args(&args(&["-help"])),
            Ok(CliAction::DelegateToKak(args(&["-help"])))
        );
    }

    #[test]
    fn test_passthrough_ro() {
        assert_eq!(
            parse_cli_args(&args(&["-ro", "file.txt"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: args(&["-ro", "file.txt"]),
            })
        );
    }

    #[test]
    fn test_passthrough_n() {
        assert_eq!(
            parse_cli_args(&args(&["-n", "file.txt"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: args(&["-n", "file.txt"]),
            })
        );
    }

    #[test]
    fn test_eval_arg_consumed() {
        assert_eq!(
            parse_cli_args(&args(&["-e", "buffer-next"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: args(&["-e", "buffer-next"]),
            })
        );
    }

    #[test]
    fn test_eval_consumes_non_ui_flag() {
        // -l is the argument to -e, not a non-UI flag
        assert_eq!(
            parse_cli_args(&args(&["-e", "-l"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: None,
                kak_args: args(&["-e", "-l"]),
            })
        );
    }

    #[test]
    fn test_double_dash_passthrough() {
        assert_eq!(
            parse_cli_args(&args(&["--ui", "gui", "--", "-e", "hello"])),
            Ok(CliAction::RunKasane {
                session: None,
                ui_mode: Some(UiMode::Gui),
                kak_args: args(&["-e", "hello"]),
            })
        );
    }

    #[test]
    fn test_conflict_ui_and_list() {
        assert_eq!(
            parse_cli_args(&args(&["--ui", "gui", "-l"])),
            Err(CliError::ConflictingFlags {
                kasane_flag: "--ui"
            })
        );
    }

    #[test]
    fn test_conflict_version_and_filter() {
        assert_eq!(
            parse_cli_args(&args(&["--version", "-f", "gg"])),
            Err(CliError::ConflictingFlags {
                kasane_flag: "--version"
            })
        );
    }

    #[test]
    fn test_unknown_ui_mode() {
        assert_eq!(
            parse_cli_args(&args(&["--ui", "ncurses"])),
            Err(CliError::UnknownUiMode("ncurses".to_string()))
        );
    }

    #[test]
    fn test_missing_ui_arg() {
        assert_eq!(
            parse_cli_args(&args(&["--ui"])),
            Err(CliError::MissingUiArg)
        );
    }

    #[test]
    fn test_plugin_new_default_template() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "new", "my-widget"])),
            Ok(CliAction::Plugin(PluginSubcommand::New {
                name: "my-widget".to_string(),
                template: PluginTemplate::Contribution,
            }))
        );
    }

    #[test]
    fn test_plugin_new_annotation_template() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "new", "x", "--template", "annotation"])),
            Ok(CliAction::Plugin(PluginSubcommand::New {
                name: "x".to_string(),
                template: PluginTemplate::Annotation,
            }))
        );
    }

    #[test]
    fn test_plugin_build_no_path() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "build"])),
            Ok(CliAction::Plugin(PluginSubcommand::Build { path: None }))
        );
    }

    #[test]
    fn test_plugin_build_with_path() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "build", "./foo"])),
            Ok(CliAction::Plugin(PluginSubcommand::Build {
                path: Some("./foo".to_string()),
            }))
        );
    }

    #[test]
    fn test_plugin_install() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "install"])),
            Ok(CliAction::Plugin(PluginSubcommand::Install { path: None }))
        );
    }

    #[test]
    fn test_plugin_list() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "list"])),
            Ok(CliAction::Plugin(PluginSubcommand::List))
        );
    }

    #[test]
    fn test_plugin_missing_subcommand() {
        assert_eq!(
            parse_cli_args(&args(&["plugin"])),
            Err(CliError::PluginMissingSubcommand)
        );
    }

    #[test]
    fn test_plugin_unknown_subcommand() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "foo"])),
            Err(CliError::PluginUnknownSubcommand("foo".to_string()))
        );
    }

    #[test]
    fn test_plugin_missing_name() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "new"])),
            Err(CliError::PluginMissingName)
        );
    }

    #[test]
    fn test_plugin_unknown_template() {
        assert_eq!(
            parse_cli_args(&args(&["plugin", "new", "x", "--template", "bad"])),
            Err(CliError::PluginUnknownTemplate("bad".to_string()))
        );
    }
}
