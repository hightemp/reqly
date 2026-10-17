//! `.netrc` lookup.

/// Login and password for `host` (or the `default` entry).
pub fn lookup(text: &str, host: &str) -> Option<(String, String)> {
    let mut tokens = text.split_whitespace().peekable();
    let mut current: Option<String> = None; // machine name; "" = default
    let mut found: Option<(String, String)> = None;
    let mut default: Option<(String, String)> = None;
    let mut login = String::new();
    let mut password = String::new();
    let mut flush = |machine: &Option<String>, login: &mut String, password: &mut String| {
        if let Some(m) = machine {
            let entry = (std::mem::take(login), std::mem::take(password));
            if m.is_empty() {
                default.get_or_insert(entry);
            } else if m.eq_ignore_ascii_case(host) && found.is_none() {
                found = Some(entry);
            }
        }
        login.clear();
        password.clear();
    };
    while let Some(tok) = tokens.next() {
        match tok {
            "machine" => {
                flush(&current, &mut login, &mut password);
                current = tokens.next().map(str::to_string);
            }
            "default" => {
                flush(&current, &mut login, &mut password);
                current = Some(String::new());
            }
            "login" => login = tokens.next().unwrap_or("").to_string(),
            "password" => password = tokens.next().unwrap_or("").to_string(),
            "account" => {
                tokens.next();
            }
            "macdef" => {
                // A macro runs until an empty line; skip its name and body
                // by treating the rest of the tokens up to the next keyword.
                tokens.next();
                while let Some(next) = tokens.peek() {
                    if matches!(*next, "machine" | "default") {
                        break;
                    }
                    tokens.next();
                }
            }
            _ => {}
        }
    }
    flush(&current, &mut login, &mut password);
    found.or(default)
}

/// `~/.netrc` (`%USERPROFILE%\_netrc` on Windows).
pub fn default_path() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(|h| std::path::PathBuf::from(h).join("_netrc"))
    } else {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".netrc"))
    }
}

#[cfg(test)]
mod tests {
    use super::lookup;

    #[test]
    fn machines_and_default() {
        let text = "machine api.example.com login alice password s3cret\n\
                    machine other.test\n  login bob\n  password pw\n\
                    default login anon password guest\n";
        assert_eq!(
            lookup(text, "api.example.com"),
            Some(("alice".into(), "s3cret".into()))
        );
        assert_eq!(
            lookup(text, "OTHER.test"),
            Some(("bob".into(), "pw".into()))
        );
        assert_eq!(
            lookup(text, "unknown.test"),
            Some(("anon".into(), "guest".into()))
        );
        assert_eq!(lookup("machine a login x password y", "b"), None);
    }
}
