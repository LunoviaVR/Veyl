//! Optional redaction of personal identifiers in exported diagnostics.

/// Identifiers to remove. Empty fields are ignored.
#[derive(Clone, Debug, Default)]
pub struct RedactionContext {
    pub user_name: String,
    pub computer_name: String,
    /// e.g. `C:\Users\alice`
    pub user_profile: String,
}

/// Replaces the user profile path, user name, computer name and user SIDs
/// (`S-1-5-21-…`) with placeholders. Matching is case-insensitive because
/// Windows paths are.
pub fn redact(text: &str, ctx: &RedactionContext) -> String {
    let mut out = text.to_owned();
    for (needle, repl) in [
        (ctx.user_profile.as_str(), "%USERPROFILE%"),
        (ctx.user_name.as_str(), "<user>"),
        (ctx.computer_name.as_str(), "<computer>"),
    ] {
        if needle.len() >= 2 {
            out = replace_ci(&out, needle, repl);
        }
    }
    redact_sids(&out)
}

fn replace_ci(haystack: &str, needle: &str, repl: &str) -> String {
    let lower_h = haystack.to_lowercase();
    let lower_n = needle.to_lowercase();
    // Lowercasing can change byte lengths for non-ASCII text; fall back to exact matching then.
    if lower_h.len() != haystack.len() || lower_n.len() != needle.len() {
        return haystack.replace(needle, repl);
    }
    let mut out = String::with_capacity(haystack.len());
    let mut i = 0;
    while let Some(pos) = lower_h[i..].find(&lower_n) {
        out.push_str(&haystack[i..i + pos]);
        out.push_str(repl);
        i += pos + needle.len();
    }
    out.push_str(&haystack[i..]);
    out
}

fn redact_sids(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(pos) = rest.find("S-1-5-21-") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 9..];
        let len = tail
            .find(|c: char| !(c.is_ascii_digit() || c == '-'))
            .unwrap_or(tail.len());
        out.push_str("S-1-5-21-<redacted>");
        rest = &tail[len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> RedactionContext {
        RedactionContext {
            user_name: "alice".into(),
            computer_name: "GAMINGPC".into(),
            user_profile: r"C:\Users\alice".into(),
        }
    }

    #[test]
    fn redacts_profile_before_name() {
        let r = redact(r"c:\users\ALICE\AppData\x.dll on gamingpc by alice", &ctx());
        assert_eq!(r, r"%USERPROFILE%\AppData\x.dll on <computer> by <user>");
    }

    #[test]
    fn redacts_sids() {
        assert_eq!(
            redact("UserID='S-1-5-21-197-229-700-1001'", &ctx()),
            "UserID='S-1-5-21-<redacted>'"
        );
        assert_eq!(redact("S-1-5-18", &ctx()), "S-1-5-18");
    }

    #[test]
    fn empty_context_is_identity_except_sids() {
        assert_eq!(redact("hello", &RedactionContext::default()), "hello");
    }
}
