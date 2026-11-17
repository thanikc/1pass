//! Previewing and importing a `.env` file into one environment. Pure over plain data: the
//! file is read and remembered by the commands layer, and values never leave this module
//! except into the vault.

use onepass_core::{
    Error, dotenv,
    protocol::{REF_PREFIX, validate_name},
    vault::{Body, Entry},
};
use serde::Serialize;
use std::collections::HashMap;
use zeroize::Zeroizing;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Secret,
    Literal,
    Reference,
    Empty,
}

impl Status {
    fn importable(self) -> bool {
        matches!(self, Self::Secret | Self::Literal)
    }
}

/// One variable of the file. Deliberately has no value field.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub name: String,
    pub status: Status,
    pub replaces: bool,
    pub preselected: bool,
}

fn status(name: &str, value: &str) -> Status {
    if value.starts_with(REF_PREFIX) {
        Status::Reference
    } else if value.is_empty() {
        Status::Empty
    } else if dotenv::looks_secret(name, value) {
        Status::Secret
    } else {
        Status::Literal
    }
}

/// One entry per name, in order of first appearance; the last value of a duplicate wins,
/// as with `op run`. Values are wiped on drop.
fn variables(text: &str) -> Result<Vec<(String, Zeroizing<String>)>, Error> {
    let mut out: Vec<(String, Zeroizing<String>)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (name, value) in dotenv::parse(text).map_err(Error::InvalidInput)? {
        let value = Zeroizing::new(value);
        match index.get(&name) {
            Some(&i) => out[i].1 = value,
            None => {
                index.insert(name.clone(), out.len());
                out.push((name, value));
            }
        }
    }
    Ok(out)
}

pub fn candidates(
    text: &str,
    body: &Body,
    project: &str,
    environment: &str,
) -> Result<Vec<Candidate>, Error> {
    let existing = body.projects.get(project).and_then(|p| p.get(environment));
    Ok(variables(text)?
        .into_iter()
        .map(|(name, value)| {
            let status = status(&name, &value);
            let replaces = existing.is_some_and(|e| e.contains_key(&name));
            Candidate {
                preselected: status == Status::Secret && !replaces,
                name,
                status,
                replaces,
            }
        })
        .collect())
}

/// All-or-nothing: every name is checked before the first entry is stored. An empty `names`
/// is refused, as there is nothing to import.
pub fn import(
    body: &mut Body,
    project: &str,
    environment: &str,
    text: &str,
    names: &[String],
) -> Result<(), Error> {
    if names.is_empty() {
        return Err(Error::InvalidInput("no variables chosen".into()));
    }
    if !body
        .projects
        .get(project)
        .is_some_and(|p| p.contains_key(environment))
    {
        return Err(Error::InvalidInput("environment not found".into()));
    }
    let vars = variables(text)?;
    let mut chosen = Vec::new();
    for name in names {
        let bad = |why: &str| Error::InvalidInput(format!("{name}: {why}"));
        validate_name(name)?;
        let (_, value) = vars
            .iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| bad("not in the file"))?;
        if !status(name, value).importable() {
            return Err(bad("cannot be imported (empty or already a reference)"));
        }
        chosen.push((name, value));
    }
    for (name, value) in chosen {
        // save_entry resets notes when given a value, so carry the existing ones over.
        let notes = body.projects[project][environment]
            .get(name)
            .map(|e: &Entry| e.notes.clone())
            .unwrap_or_default();
        body.save_entry(project, environment, name, None, Some(value.clone()), notes)?;
    }
    Ok(())
}

/// `NAME=op://project/environment/NAME` lines for the user to paste, sorted by name.
pub fn reference_lines(project: &str, environment: &str, names: &[String]) -> String {
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort();
    sorted
        .iter()
        .map(|n| format!("{n}={REF_PREFIX}{project}/{environment}/{n}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "STRIPE_SECRET=sk_live_abc\nLOG_LEVEL=debug\nAPI_URL=op://myapp/dev/API_URL\nEMPTY=\nDATABASE_URL=postgres://u:p@host/db\n";

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    fn vault() -> Body {
        let mut b = Body::default();
        b.save_entry(
            "myapp",
            "dev",
            "DATABASE_URL",
            None,
            Some(Zeroizing::new("old".into())),
            "primary db".into(),
        )
        .unwrap();
        b
    }

    fn value(b: &Body, n: &str) -> Option<String> {
        b.value("myapp", "dev", n).ok().map(|v| v.to_string())
    }

    #[test]
    fn candidates_have_status_replaces_and_preselection() {
        let c = candidates(FILE, &vault(), "myapp", "dev").unwrap();
        let got: Vec<_> = c
            .iter()
            .map(|c| (c.name.as_str(), c.status, c.replaces, c.preselected))
            .collect();
        assert_eq!(
            got,
            [
                ("STRIPE_SECRET", Status::Secret, false, true),
                ("LOG_LEVEL", Status::Literal, false, false),
                ("API_URL", Status::Reference, false, false),
                ("EMPTY", Status::Empty, false, false),
                ("DATABASE_URL", Status::Literal, true, false),
            ]
        );
    }

    #[test]
    fn candidates_carry_no_value() {
        let c = candidates(FILE, &vault(), "myapp", "dev").unwrap();
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("sk_live_abc") && !json.contains("postgres"));
        assert!(json.contains("\"status\":\"secret\""));
    }

    #[test]
    fn chosen_candidates_become_entries_with_file_values() {
        let mut b = vault();
        import(
            &mut b,
            "myapp",
            "dev",
            FILE,
            &names(&["STRIPE_SECRET", "LOG_LEVEL"]),
        )
        .unwrap();
        assert_eq!(value(&b, "STRIPE_SECRET").unwrap(), "sk_live_abc");
        assert_eq!(value(&b, "LOG_LEVEL").unwrap(), "debug");
    }

    #[test]
    fn reference_lines_are_sorted_by_name() {
        assert_eq!(
            reference_lines("myapp", "dev", &names(&["STRIPE_SECRET", "LOG_LEVEL"])),
            "LOG_LEVEL=op://myapp/dev/LOG_LEVEL\nSTRIPE_SECRET=op://myapp/dev/STRIPE_SECRET"
        );
    }

    #[test]
    fn chosen_existing_entry_is_replaced_and_keeps_notes() {
        let mut b = vault();
        import(&mut b, "myapp", "dev", FILE, &names(&["DATABASE_URL"])).unwrap();
        assert_eq!(value(&b, "DATABASE_URL").unwrap(), "postgres://u:p@host/db");
        assert_eq!(
            b.projects["myapp"]["dev"]["DATABASE_URL"].notes,
            "primary db"
        );
    }

    #[test]
    fn unchosen_existing_entry_is_untouched() {
        let mut b = vault();
        import(&mut b, "myapp", "dev", FILE, &names(&["STRIPE_SECRET"])).unwrap();
        assert_eq!(value(&b, "DATABASE_URL").unwrap(), "old");
    }

    #[test]
    fn a_name_that_cannot_be_imported_stores_nothing() {
        for bad in ["API_URL", "EMPTY", "MISSING"] {
            let mut b = vault();
            let r = import(
                &mut b,
                "myapp",
                "dev",
                FILE,
                &names(&["STRIPE_SECRET", bad]),
            );
            assert!(matches!(r, Err(Error::InvalidInput(_))), "{bad}");
            assert!(value(&b, "STRIPE_SECRET").is_none(), "{bad}");
        }
    }

    #[test]
    fn errors_name_the_variable_but_never_the_value() {
        let mut b = vault();
        let e = import(&mut b, "myapp", "dev", FILE, &names(&["API_URL"])).unwrap_err();
        assert!(e.to_string().contains("API_URL") && !e.to_string().contains("op://"));
    }

    #[test]
    fn later_duplicates_win() {
        let text = "TOKEN=first\nOTHER=x\nTOKEN=second";
        let c = candidates(text, &vault(), "myapp", "dev").unwrap();
        assert_eq!(c.iter().filter(|c| c.name == "TOKEN").count(), 1);
        let mut b = vault();
        import(&mut b, "myapp", "dev", text, &names(&["TOKEN"])).unwrap();
        assert_eq!(value(&b, "TOKEN").unwrap(), "second");
    }

    #[test]
    fn dotenv_syntax_error_is_refused() {
        let r = candidates("STRIPE_SECRET=ok\nnot a pair", &vault(), "myapp", "dev");
        assert!(matches!(r, Err(Error::InvalidInput(m)) if m.contains("line 2")));
        let mut b = vault();
        let r = import(&mut b, "myapp", "dev", "oops", &names(&["X"]));
        assert!(matches!(r, Err(Error::InvalidInput(_))));
    }

    #[test]
    fn file_changed_since_preview_fails() {
        let mut b = vault();
        let r = import(&mut b, "myapp", "dev", "OTHER=1", &names(&["TOKEN"]));
        assert!(matches!(r, Err(Error::InvalidInput(_))));
        assert!(value(&b, "TOKEN").is_none());
    }

    #[test]
    fn empty_choice_and_missing_environment_are_refused() {
        let mut b = vault();
        assert!(import(&mut b, "myapp", "dev", FILE, &[]).is_err());
        let r = import(&mut b, "myapp", "nope", FILE, &names(&["LOG_LEVEL"]));
        assert!(r.is_err() && !b.projects["myapp"].contains_key("nope"));
    }
}
