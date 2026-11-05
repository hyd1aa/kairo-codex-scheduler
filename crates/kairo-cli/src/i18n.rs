use std::io::{self, IsTerminal};
pub fn t<'a>(en: bool, zh: &'a str, english: &'a str) -> &'a str {
    if en {
        english
    } else {
        zh
    }
}
pub fn color(s: &str, code: u8) -> String {
    if io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var("TERM").as_deref() != Ok("dumb")
    {
        format!("\x1b[{code}m{s}\x1b[0m")
    } else {
        s.into()
    }
}
pub fn bilingual(s: &str, en: bool) -> String {
    if let Some((a, b)) = s.split_once(" / ") {
        if en {
            a.replace('_', " ")
        } else {
            b.into()
        }
    } else {
        s.into()
    }
}
pub fn error(e: &anyhow::Error, en: bool) -> (String, String) {
    let text = e.to_string();
    if let Some((code, _)) = text.split_once(" / ") {
        return (
            code.to_lowercase().replace([' ', '/'], "_"),
            bilingual(&text, en),
        );
    }
    let code = if e.chain().any(|e| e.is::<std::io::Error>()) {
        "io_error"
    } else if e.chain().any(|e| e.is::<serde_json::Error>()) {
        "json_invalid"
    } else {
        "input_invalid"
    };
    (
        code.into(),
        t(
            en,
            "操作失败，请检查参数、文件权限或诊断信息",
            "Operation failed; check arguments, file permissions or diagnostics",
        )
        .into(),
    )
}
