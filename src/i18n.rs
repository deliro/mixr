pub struct Locale {
    pub source: &'static str,
    pub destination: &'static str,
    pub size: &'static str,
    pub min_size: &'static str,
    pub extensions: &'static str,
    pub exclude: &'static str,
    pub no_live: &'static str,
    pub keep_names: &'static str,
    pub overwrite: &'static str,
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

    pub min_duration: &'static str,
    pub encoding_label: &'static str,
    pub bitrate_label: &'static str,
    pub quality_label: &'static str,
    pub ph_min_duration: &'static str,
    pub keep_original: &'static str,
    pub quality_high: &'static str,
    pub quality_medium: &'static str,
    pub quality_low: &'static str,
    pub err_invalid_duration: &'static str,
    pub err_bitrate_required: &'static str,
    pub preparing: &'static str,
    pub converting: &'static str,

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
    no_live: "No live:",
    keep_names: "Keep names:",
    overwrite: "Overwrite:",
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

    min_duration: "Min duration:",
    encoding_label: "Encoding:",
    bitrate_label: "Bitrate:",
    quality_label: "Quality:",
    ph_min_duration: "30s, 2m, 2:30",
    keep_original: "Keep original",
    quality_high: "High (~245kbps)",
    quality_medium: "Medium (~190kbps)",
    quality_low: "Low (~130kbps)",
    err_invalid_duration: "Invalid duration format",
    err_bitrate_required: "Bitrate is required for CBR",
    preparing: "preparing\u{2026}",
    converting: "converting\u{2026}",

    err_source_not_found: "Error: source not found",
    err_both_required: "Error: both SOURCE and DESTINATION are required in CLI mode",
    err_usage: "Usage: mixr [OPTIONS] <SOURCE> <DESTINATION>",
    err_run_tui: "Run without arguments for interactive TUI mode",
};

pub static RU: Locale = Locale {
    source: "Источник:",
    destination: "Назначение:",
    size: "Размер:",
    min_size: "Мин. размер:",
    extensions: "Расширения:",
    exclude: "Исключить:",
    no_live: "Без live:",
    keep_names: "Сохр. имена:",
    overwrite: "Перезаписать:",
    start: "Старт",

    ph_source: "~/Музыка",
    ph_destination: "Ctrl+D \u{2014} диски",
    ph_size: "авто (свободное место)",
    ph_min_size: "напр. 1M",
    ph_extensions: "mp3, flac, ogg, ...",
    ph_exclude: "напр. wav, wma",

    help_setup: "\u{2191}\u{2193}: навигация  Tab: дополнить  ^D: диски  Enter: запуск  ^C: выход",

    err_source_required: "Укажите путь к источнику",
    err_dest_required: "Укажите путь назначения",
    err_source_not_dir: "Источник не является папкой",
    err_invalid_size: "Неверный размер",
    err_invalid_min_size: "Неверный мин. размер",

    scanning: "Сканирование",
    found: "найдено",
    matched: "подходит",

    copying: "Копирование",
    current: "Текущий",
    total: "Всего",
    elapsed: "Прошло",
    eta: "Осталось",
    ctrl_c_stop: "Ctrl+C для остановки",

    done: "Готово!",
    copied_files: "Скопировано",
    time: "Время",
    errors: "ошибок",
    press_q: "Нажмите q для выхода",

    fatal_error: "Критическая ошибка!",

    cli_done: "Готово",
    cli_copying: "Копирование",
    cli_fatal: "Критическая ошибка",
    cli_errors: "ошибок",

    about: "Заполни флешку случайной музыкой",

    min_duration: "Мин. длит.:",
    encoding_label: "Кодирование:",
    bitrate_label: "Битрейт:",
    quality_label: "Качество:",
    ph_min_duration: "30s, 2m, 2:30",
    keep_original: "Без изменений",
    quality_high: "Высокое (~245kbps)",
    quality_medium: "Среднее (~190kbps)",
    quality_low: "Низкое (~130kbps)",
    err_invalid_duration: "Некорректная длительность",
    err_bitrate_required: "Битрейт обязателен для CBR",
    preparing: "подготовка\u{2026}",
    converting: "конвертация\u{2026}",

    err_source_not_found: "Ошибка: источник не найден",
    err_both_required: "Ошибка: в режиме CLI необходимы SOURCE и DESTINATION",
    err_usage: "Использование: mixr [OPTIONS] <SOURCE> <DESTINATION>",
    err_run_tui: "Запустите без аргументов для интерактивного режима TUI",
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
