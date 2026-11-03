//! Host names from `~/.ssh/config`, offered in the "Add remote folder" dialog.

use std::path::Path;

/// The `Host` names in an ssh config text, without patterns (`*`, `?`, `!`),
/// in file order, each once.
pub fn host_names(config: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once(|character: char| character.is_whitespace() || character == '=')
        else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("host") {
            continue;
        }
        for name in rest.split_whitespace().map(|name| name.trim_matches('"')) {
            let is_pattern = name.contains(['*', '?', '!']);
            if !is_pattern && super::mirror::is_valid_host(name) && !names.iter().any(|known| known == name) {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// The host names in the user's `~/.ssh/config` (none when it is missing).
pub fn user_host_names() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    std::fs::read_to_string(Path::new(&home).join(".ssh/config")).map(|text| host_names(&text)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_host_names() {
        let config = "Host *\n  ForwardAgent no\nHost notes work-box\n  HostName 10.0.0.2\n\
            host=lab\nHost *.example.com !bad\n# Host commented\nMatch host x\nHost notes\n";
        assert_eq!(host_names(config), ["notes", "work-box", "lab"]);
    }
}
