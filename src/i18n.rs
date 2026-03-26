pub struct Locale {
    pub source: &'static str,
    pub destination: &'static str,
    pub size: &'static str,
    pub min_size: &'static str,
    pub extensions: &'static str,
    pub exclude: &'static str,
    pub no_live: &'static str,
    pub keep_names: &'static str,
    pub start: &'static str,

    pub ph_source: &'static str,
    pub ph_destination: &'static str,
    pub ph_size: &'static str,
    pub ph_min_size: &'static str,
    pub ph_extensions: &'static str,
    pub ph_exclude: &'static str,

    pub help_setup: &'static str,

    pub err_source_required: &'static str,
    pub err_dest_required: &'static str,
    pub err_source_not_dir: &'static str,
    pub err_invalid_size: &'static str,
    pub err_invalid_min_size: &'static str,

    pub scanning: &'static str,
    pub found: &'static str,
    pub matched: &'static str,

    pub copying: &'static str,
    pub current: &'static str,
    pub total: &'static str,
    pub elapsed: &'static str,
    pub eta: &'static str,
    pub ctrl_c_stop: &'static str,

    pub done: &'static str,
    pub copied_files: &'static str,
    pub time: &'static str,
    pub errors: &'static str,
    pub press_q: &'static str,

    pub fatal_error: &'static str,

    pub cli_done: &'static str,
    pub cli_copying: &'static str,
    pub cli_fatal: &'static str,
    pub cli_errors: &'static str,

    #[allow(dead_code)]
    pub about: &'static str,

    pub err_source_not_found: &'static str,
    pub err_both_required: &'static str,
    pub err_usage: &'static str,
    pub err_run_tui: &'static str,
}

pub static EN: Locale = Locale {
    source: "Source:",
    destination: "Destination:",
    size: "Size:",
    min_size: "Min size:",
    extensions: "Extensions:",
    exclude: "Exclude:",
    no_live: "No live",
    keep_names: "Keep names",
    start: "Start",

    ph_source: "~/Music",
    ph_destination: "Ctrl+D for drives",
    ph_size: "auto (free space)",
    ph_min_size: "e.g. 1M",
    ph_extensions: "mp3, flac, ogg, ...",
    ph_exclude: "e.g. wav, wma",

    help_setup: "\u{2191}\u{2193}: navigate  Tab: complete  ^D: drives  Enter: go  ^C: quit",

    err_source_required: "Source path is required",
    err_dest_required: "Destination path is required",
    err_source_not_dir: "Source is not a directory",
    err_invalid_size: "Invalid size",
    err_invalid_min_size: "Invalid min size",

    scanning: "Scanning",
    found: "found",
    matched: "matched",

    copying: "Copying",
    current: "Current",
    total: "Total",
    elapsed: "Elapsed",
    eta: "ETA",
    ctrl_c_stop: "Ctrl+C to stop",

    done: "Done!",
    copied_files: "Copied",
    time: "Time",
    errors: "errors",
    press_q: "Press q to quit",

    fatal_error: "Fatal error!",

    cli_done: "Done",
    cli_copying: "Copying",
    cli_fatal: "Fatal error",
    cli_errors: "errors",

    about: "Fill your flash drive with random music",

    err_source_not_found: "Error: source not found",
    err_both_required: "Error: both SOURCE and DESTINATION are required in CLI mode",
    err_usage: "Usage: mixr [OPTIONS] <SOURCE> <DESTINATION>",
    err_run_tui: "Run without arguments for interactive TUI mode",
};

pub static RU: Locale = Locale {
    source: "\u{0418}\u{0441}\u{0442}\u{043E}\u{0447}\u{043D}\u{0438}\u{043A}:",
    destination: "\u{041D}\u{0430}\u{0437}\u{043D}\u{0430}\u{0447}\u{0435}\u{043D}\u{0438}\u{0435}:",
    size: "\u{0420}\u{0430}\u{0437}\u{043C}\u{0435}\u{0440}:",
    min_size: "\u{041C}\u{0438}\u{043D}. \u{0440}\u{0430}\u{0437}\u{043C}\u{0435}\u{0440}:",
    extensions: "\u{0420}\u{0430}\u{0441}\u{0448}\u{0438}\u{0440}\u{0435}\u{043D}\u{0438}\u{044F}:",
    exclude: "\u{0418}\u{0441}\u{043A}\u{043B}\u{044E}\u{0447}\u{0438}\u{0442}\u{044C}:",
    no_live: "\u{0411}\u{0435}\u{0437} live",
    keep_names: "\u{0421}\u{043E}\u{0445}\u{0440}\u{0430}\u{043D}\u{0438}\u{0442}\u{044C} \u{0438}\u{043C}\u{0435}\u{043D}\u{0430}",
    start: "\u{0421}\u{0442}\u{0430}\u{0440}\u{0442}",

    ph_source: "~/\u{041C}\u{0443}\u{0437}\u{044B}\u{043A}\u{0430}",
    ph_destination: "Ctrl+D \u{2014} \u{0434}\u{0438}\u{0441}\u{043A}\u{0438}",
    ph_size: "\u{0430}\u{0432}\u{0442}\u{043E} (\u{0441}\u{0432}\u{043E}\u{0431}\u{043E}\u{0434}\u{043D}\u{043E}\u{0435} \u{043C}\u{0435}\u{0441}\u{0442}\u{043E})",
    ph_min_size: "\u{043D}\u{0430}\u{043F}\u{0440}. 1M",
    ph_extensions: "mp3, flac, ogg, ...",
    ph_exclude: "\u{043D}\u{0430}\u{043F}\u{0440}. wav, wma",

    help_setup: "\u{2191}\u{2193}: \u{043D}\u{0430}\u{0432}\u{0438}\u{0433}\u{0430}\u{0446}\u{0438}\u{044F}  Tab: \u{0434}\u{043E}\u{043F}\u{043E}\u{043B}\u{043D}\u{0438}\u{0442}\u{044C}  ^D: \u{0434}\u{0438}\u{0441}\u{043A}\u{0438}  Enter: \u{0437}\u{0430}\u{043F}\u{0443}\u{0441}\u{043A}  ^C: \u{0432}\u{044B}\u{0445}\u{043E}\u{0434}",

    err_source_required: "\u{0423}\u{043A}\u{0430}\u{0436}\u{0438}\u{0442}\u{0435} \u{043F}\u{0443}\u{0442}\u{044C} \u{043A} \u{0438}\u{0441}\u{0442}\u{043E}\u{0447}\u{043D}\u{0438}\u{043A}\u{0443}",
    err_dest_required: "\u{0423}\u{043A}\u{0430}\u{0436}\u{0438}\u{0442}\u{0435} \u{043F}\u{0443}\u{0442}\u{044C} \u{043D}\u{0430}\u{0437}\u{043D}\u{0430}\u{0447}\u{0435}\u{043D}\u{0438}\u{044F}",
    err_source_not_dir: "\u{0418}\u{0441}\u{0442}\u{043E}\u{0447}\u{043D}\u{0438}\u{043A} \u{043D}\u{0435} \u{044F}\u{0432}\u{043B}\u{044F}\u{0435}\u{0442}\u{0441}\u{044F} \u{043F}\u{0430}\u{043F}\u{043A}\u{043E}\u{0439}",
    err_invalid_size: "\u{041D}\u{0435}\u{0432}\u{0435}\u{0440}\u{043D}\u{044B}\u{0439} \u{0440}\u{0430}\u{0437}\u{043C}\u{0435}\u{0440}",
    err_invalid_min_size: "\u{041D}\u{0435}\u{0432}\u{0435}\u{0440}\u{043D}\u{044B}\u{0439} \u{043C}\u{0438}\u{043D}. \u{0440}\u{0430}\u{0437}\u{043C}\u{0435}\u{0440}",

    scanning: "\u{0421}\u{043A}\u{0430}\u{043D}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{043D}\u{0438}\u{0435}",
    found: "\u{043D}\u{0430}\u{0439}\u{0434}\u{0435}\u{043D}\u{043E}",
    matched: "\u{043F}\u{043E}\u{0434}\u{0445}\u{043E}\u{0434}\u{0438}\u{0442}",

    copying: "\u{041A}\u{043E}\u{043F}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{043D}\u{0438}\u{0435}",
    current: "\u{0422}\u{0435}\u{043A}\u{0443}\u{0449}\u{0438}\u{0439}",
    total: "\u{0412}\u{0441}\u{0435}\u{0433}\u{043E}",
    elapsed: "\u{041F}\u{0440}\u{043E}\u{0448}\u{043B}\u{043E}",
    eta: "\u{041E}\u{0441}\u{0442}\u{0430}\u{043B}\u{043E}\u{0441}\u{044C}",
    ctrl_c_stop: "Ctrl+C \u{0434}\u{043B}\u{044F} \u{043E}\u{0441}\u{0442}\u{0430}\u{043D}\u{043E}\u{0432}\u{043A}\u{0438}",

    done: "\u{0413}\u{043E}\u{0442}\u{043E}\u{0432}\u{043E}!",
    copied_files: "\u{0421}\u{043A}\u{043E}\u{043F}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{043D}\u{043E}",
    time: "\u{0412}\u{0440}\u{0435}\u{043C}\u{044F}",
    errors: "\u{043E}\u{0448}\u{0438}\u{0431}\u{043E}\u{043A}",
    press_q: "\u{041D}\u{0430}\u{0436}\u{043C}\u{0438}\u{0442}\u{0435} q \u{0434}\u{043B}\u{044F} \u{0432}\u{044B}\u{0445}\u{043E}\u{0434}\u{0430}",

    fatal_error: "\u{041A}\u{0440}\u{0438}\u{0442}\u{0438}\u{0447}\u{0435}\u{0441}\u{043A}\u{0430}\u{044F} \u{043E}\u{0448}\u{0438}\u{0431}\u{043A}\u{0430}!",

    cli_done: "\u{0413}\u{043E}\u{0442}\u{043E}\u{0432}\u{043E}",
    cli_copying: "\u{041A}\u{043E}\u{043F}\u{0438}\u{0440}\u{043E}\u{0432}\u{0430}\u{043D}\u{0438}\u{0435}",
    cli_fatal: "\u{041A}\u{0440}\u{0438}\u{0442}\u{0438}\u{0447}\u{0435}\u{0441}\u{043A}\u{0430}\u{044F} \u{043E}\u{0448}\u{0438}\u{0431}\u{043A}\u{0430}",
    cli_errors: "\u{043E}\u{0448}\u{0438}\u{0431}\u{043E}\u{043A}",

    about: "\u{0417}\u{0430}\u{043F}\u{043E}\u{043B}\u{043D}\u{0438} \u{0444}\u{043B}\u{0435}\u{0448}\u{043A}\u{0443} \u{0441}\u{043B}\u{0443}\u{0447}\u{0430}\u{0439}\u{043D}\u{043E}\u{0439} \u{043C}\u{0443}\u{0437}\u{044B}\u{043A}\u{043E}\u{0439}",

    err_source_not_found: "\u{041E}\u{0448}\u{0438}\u{0431}\u{043A}\u{0430}: \u{0438}\u{0441}\u{0442}\u{043E}\u{0447}\u{043D}\u{0438}\u{043A} \u{043D}\u{0435} \u{043D}\u{0430}\u{0439}\u{0434}\u{0435}\u{043D}",
    err_both_required: "\u{041E}\u{0448}\u{0438}\u{0431}\u{043A}\u{0430}: \u{0432} \u{0440}\u{0435}\u{0436}\u{0438}\u{043C}\u{0435} CLI \u{043D}\u{0435}\u{043E}\u{0431}\u{0445}\u{043E}\u{0434}\u{0438}\u{043C}\u{044B} SOURCE \u{0438} DESTINATION",
    err_usage: "\u{0418}\u{0441}\u{043F}\u{043E}\u{043B}\u{044C}\u{0437}\u{043E}\u{0432}\u{0430}\u{043D}\u{0438}\u{0435}: mixr [OPTIONS] <SOURCE> <DESTINATION>",
    err_run_tui: "\u{0417}\u{0430}\u{043F}\u{0443}\u{0441}\u{0442}\u{0438}\u{0442}\u{0435} \u{0431}\u{0435}\u{0437} \u{0430}\u{0440}\u{0433}\u{0443}\u{043C}\u{0435}\u{043D}\u{0442}\u{043E}\u{0432} \u{0434}\u{043B}\u{044F} \u{0438}\u{043D}\u{0442}\u{0435}\u{0440}\u{0430}\u{043A}\u{0442}\u{0438}\u{0432}\u{043D}\u{043E}\u{0433}\u{043E} \u{0440}\u{0435}\u{0436}\u{0438}\u{043C}\u{0430} TUI",
};

pub fn detect() -> &'static Locale {
    if let Ok(lang) = std::env::var("MIXR_LANG") {
        if lang.starts_with("ru") {
            return &RU;
        }
        return &EN;
    }
    match sys_locale::get_locale() {
        Some(loc) if loc.starts_with("ru") => &RU,
        _ => &EN,
    }
}
