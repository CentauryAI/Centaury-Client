//! Spec parsers for the `--role` / `--skill` launch flags (Task 8) and the
//! hook's self-assignment from `KORE_ROLE` / `KORE_SKILLS` env (Task 10 step 1).
//! The SERVER owns cadence storage + defaults; these mirror the defaults so the
//! CLI fails fast on a malformed spec instead of shipping it to the agent.

use kore_protocol::api::SkillAssignment;

const DEFAULT_ROLE_CADENCE: i64 = 20; // messages (matches server default)
const DEFAULT_SKILL_CADENCE: i64 = 15; // messages

/// `"<title>[:every=<N><msg|tok>]"` → `(title, cadence_kind, cadence_value)`.
/// Default cadence: every 20 messages.
pub fn parse_role_spec(spec: &str) -> Result<(String, String, i64), String> {
    let mut parts = spec.split(':');
    let title = parts.next().unwrap_or("").trim();
    if title.is_empty() {
        return Err("role spec: empty title".into());
    }
    let (kind, value) = match parts.next() {
        Some(seg) => parse_every(seg)?,
        None => ("messages".into(), DEFAULT_ROLE_CADENCE),
    };
    if parts.next().is_some() {
        return Err(format!("role spec '{spec}': too many ':' segments"));
    }
    Ok((title.to_string(), kind, value))
}

/// `"<name>[:every=<N><msg|tok>][:pointer|full]"` → `SkillAssignment`.
/// Segments after the name may come in either order. Defaults: every 15
/// messages, pointer.
pub fn parse_skill_spec(spec: &str) -> Result<SkillAssignment, String> {
    let mut parts = spec.split(':');
    let name = parts.next().unwrap_or("").trim();
    if name.is_empty() {
        return Err("skill spec: empty name".into());
    }
    let mut cadence_kind = "messages".to_string();
    let mut cadence_value = DEFAULT_SKILL_CADENCE;
    let mut inject_mode = "pointer".to_string();
    for seg in parts {
        match seg {
            "pointer" | "full" => inject_mode = seg.to_string(),
            _ if seg.starts_with("every=") => {
                let (k, v) = parse_every(seg)?;
                cadence_kind = k;
                cadence_value = v;
            }
            _ => return Err(format!("skill spec '{spec}': unknown segment '{seg}'")),
        }
    }
    Ok(SkillAssignment { skill: name.to_string(), cadence_kind, cadence_value, inject_mode })
}

/// `"every=<N><msg|tok>"` → `(cadence_kind, N)`. N must be > 0.
fn parse_every(seg: &str) -> Result<(String, i64), String> {
    let rest = seg
        .strip_prefix("every=")
        .ok_or_else(|| format!("expected 'every=<N><msg|tok>', got '{seg}'"))?;
    let (num, kind) = if let Some(n) = rest.strip_suffix("msg") {
        (n, "messages")
    } else if let Some(n) = rest.strip_suffix("tok") {
        (n, "tokens")
    } else {
        return Err(format!("cadence '{seg}': unit must be 'msg' or 'tok'"));
    };
    let value: i64 = num.parse().map_err(|_| format!("cadence '{seg}': bad number"))?;
    if value <= 0 {
        return Err(format!("cadence '{seg}': must be > 0"));
    }
    Ok((kind.to_string(), value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_specs() {
        assert_eq!(
            parse_role_spec("Líder").unwrap(),
            ("Líder".into(), "messages".into(), 20)
        );
        assert_eq!(
            parse_role_spec("Líder:every=10msg").unwrap(),
            ("Líder".into(), "messages".into(), 10)
        );
        let s = parse_skill_spec("tdd:every=8000tok:full").unwrap();
        assert_eq!(
            (s.skill.as_str(), s.cadence_kind.as_str(), s.cadence_value, s.inject_mode.as_str()),
            ("tdd", "tokens", 8000, "full")
        );
        // defaults + segment order independence
        let d = parse_skill_spec("tdd").unwrap();
        assert_eq!(
            (d.cadence_kind.as_str(), d.cadence_value, d.inject_mode.as_str()),
            ("messages", 15, "pointer")
        );
        let r = parse_skill_spec("tdd:full:every=5msg").unwrap();
        assert_eq!((r.inject_mode.as_str(), r.cadence_value), ("full", 5));

        // errors
        assert!(parse_skill_spec("tdd:every=0msg").is_err());
        assert!(parse_role_spec("a:b:c").is_err());
        assert!(parse_skill_spec("tdd:bogus").is_err());
        assert!(parse_role_spec("").is_err());
        assert!(parse_role_spec("x:every=5min").is_err());
    }
}
