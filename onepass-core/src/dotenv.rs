//! Minimal dotenv: `KEY=value`, `export`, comments, single/double quotes. No interpolation.
//! shortcut: quoted values can't span lines, add multi-line support if a real .env needs it.

pub fn parse(text: &str) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |m: &str| format!("line {}: {m}", i + 1);
        let line = match line.strip_prefix("export") {
            Some(r) if r.starts_with(char::is_whitespace) => r.trim_start(),
            _ => line,
        };
        let (key, val) = line
            .split_once('=')
            .ok_or_else(|| err("expected KEY=value"))?;
        let key = key.trim();
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        {
            return Err(err("invalid variable name"));
        }
        out.push((
            key.to_string(),
            value(val.trim_start()).map_err(|m| err(m))?,
        ));
    }
    Ok(out)
}

fn value(v: &str) -> Result<String, &'static str> {
    let Some(q) = v.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
        // Unquoted: a `#` preceded by whitespace starts a comment.
        let end = v
            .char_indices()
            .find(|&(i, c)| c == '#' && v[..i].ends_with(char::is_whitespace));
        return Ok(v[..end.map_or(v.len(), |(i, _)| i)].trim_end().to_string());
    };
    let mut s = String::new();
    let mut chars = v[1..].char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            c if c == q => {
                let rest = v[i + 2..].trim_start();
                return if rest.is_empty() || rest.starts_with('#') {
                    Ok(s)
                } else {
                    Err("text after closing quote")
                };
            }
            '\\' if q == '"' => match chars.next().map(|(_, c)| c) {
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some(c @ ('"' | '\\')) => s.push(c),
                Some(c) => {
                    s.push('\\');
                    s.push(c)
                }
                None => break,
            },
            c => s.push(c),
        }
    }
    Err("unterminated quote")
}

/// Heuristic: does this *literal* value look like a secret? Only the name is ever reported.
pub fn looks_secret(name: &str, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let n = name.to_ascii_uppercase();
    if ["_KEY", "_TOKEN", "_SECRET", "_PASSWORD"]
        .iter()
        .any(|s| n.ends_with(s))
    {
        return true;
    }
    value.len() >= 24 && !value.contains(char::is_whitespace) && entropy(value) >= 4.0
}

fn entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    for c in s.chars() {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let n = s.chars().count() as f64;
    counts
        .values()
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Vec<(String, String)> {
        parse(s).unwrap()
    }
    fn kv(k: &str, v: &str) -> (String, String) {
        (k.into(), v.into())
    }

    #[test]
    fn basics() {
        assert_eq!(
            p("# c\n\nA=1\nexport B = two  # note\nC=a#b"),
            [kv("A", "1"), kv("B", "two"), kv("C", "a#b")]
        );
    }

    #[test]
    fn quotes() {
        assert_eq!(p(r#"A="x # y\n\"z\"" # c"#), [kv("A", "x # y\n\"z\"")]);
        assert_eq!(p(r#"A='a\nb $HOME'"#), [kv("A", r"a\nb $HOME")]);
        assert_eq!(p("A="), [kv("A", "")]);
        assert_eq!(p("A=\"é\""), [kv("A", "é")]);
    }

    #[test]
    fn no_interpolation() {
        assert_eq!(p("A=${B}"), [kv("A", "${B}")]);
    }

    #[test]
    fn errors() {
        for bad in ["NOEQUALS", "=x", "A B=1", "A=\"open", "A='x'y", "A=\"x\" y"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn heuristics() {
        assert!(looks_secret("STRIPE_SECRET", "x"));
        assert!(looks_secret("api_token", "x"));
        assert!(!looks_secret("API_KEY", ""));
        assert!(looks_secret("FOO", "mock_live_4eC39HqLyjWDarjtT1zdp7dc"));
        assert!(!looks_secret("URL", "http://localhost:3000/api/v1/things"));
        assert!(!looks_secret("PORT", "3000"));
    }
}
