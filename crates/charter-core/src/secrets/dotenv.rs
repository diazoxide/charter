//! One `KEY=value` line of a dotenv file — `commands_secrets._dotenv_line`.
//!
//! The consumer is the `dotenv` package's parser (Playwright's `dotenvFileLoader`), verified by
//! Python charter at v17.4.2 by exhaustive fuzz. Three tiers, the first that applies wins:
//! single quotes (fully literal), backticks (literal, and carry a real newline), double quotes
//! (the only tier that carries a CR, by escaping). A value no tier can carry is REFUSED rather
//! than silently corrupted, and the refusal names why without printing the value.

/// Why a line could not be written. The message never holds the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unrepresentable(pub String);

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn has_escape_seq(value: &str) -> bool {
    value.contains("\\n") || value.contains("\\r")
}

/// `_dotenv_line(name, value)`.
pub fn line(name: &str, value: &str) -> Result<String, Unrepresentable> {
    if !valid_name(name) {
        return Err(Unrepresentable(format!(
            "'{name}' is not a valid environment-variable name (expected [A-Za-z_][A-Za-z0-9_]*)"
        )));
    }
    let has_cr = value.contains('\r');
    let has_lf = value.contains('\n');
    let has_sq = value.contains('\'');
    if !has_cr && !(value.contains('#') && has_sq) && !(has_sq && has_lf) {
        return Ok(format!("{name}='{value}'"));
    }
    if !has_cr && !value.contains('`') {
        return Ok(format!("{name}=`{value}`"));
    }
    if !value.contains('"') && !has_escape_seq(value) {
        let body = value.replace('\r', "\\r").replace('\n', "\\n");
        return Ok(format!("{name}=\"{body}\""));
    }
    let why = if value.contains('\r') {
        "it combines a real carriage return with a double quote. Only a double-quoted value can \
         carry a carriage return, and that tier cannot also contain a '\"'"
    } else if has_escape_seq(value) {
        "it combines a real newline, a double quote and a literal '\\n'/'\\r' escape sequence, so \
         the escape would be indistinguishable from the real newline"
    } else {
        "it contains '#', a single quote, a double quote and a backtick all at once, which leaves \
         no usable quote style"
    };
    Err(Unrepresentable(format!(
        "the secret for '{name}' cannot be represented in dotenv: {why}. Store the value \
         base64-encoded instead. (Value withheld from this message.)"
    )))
}

/// The escaped form tier 3 writes, which redaction must also match.
pub fn escaped(value: &str) -> String {
    value.replace('\r', "\\r").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_value_is_single_quoted() {
        assert_eq!(line("A", "abc").unwrap(), "A='abc'");
    }

    #[test]
    fn a_hash_a_quote_or_a_newline_alone_stays_single_quoted() {
        assert_eq!(line("H", "a#b").unwrap(), "H='a#b'");
        assert_eq!(line("S", "it's").unwrap(), "S='it's'");
        assert_eq!(line("N", "a\nb").unwrap(), "N='a\nb'");
    }

    #[test]
    fn a_quote_beside_a_hash_takes_backticks() {
        assert_eq!(line("Q", "it's #x").unwrap(), "Q=`it's #x`");
    }

    #[test]
    fn a_carriage_return_takes_double_quotes_escaped() {
        assert_eq!(line("C", "a\r\nb").unwrap(), "C=\"a\\r\\nb\"");
    }

    #[test]
    fn a_value_no_tier_can_carry_is_refused_without_the_value() {
        let value = "x\"\r";
        let err = line("V", value).unwrap_err();
        assert!(err.0.contains("carriage return"));
        assert!(!err.0.contains(value));
    }

    #[test]
    fn a_newline_beside_a_quote_and_a_backtick_takes_double_quotes_escaped() {
        assert_eq!(line("N", "x\n'`y").unwrap(), "N=\"x\\n'`y\"");
    }

    #[test]
    fn a_literal_escape_beside_a_real_newline_is_refused_as_ambiguous() {
        for value in ["x\n'`\\n", "x\n'`\\r"] {
            let err = line("E", value).unwrap_err();
            assert!(err.0.contains("literal '\\n'/'\\r' escape"), "{}", err.0);
        }
    }

    #[test]
    fn every_quote_style_at_once_is_refused_as_having_none_left() {
        let err = line("Q", "'#`\"").unwrap_err();
        assert!(err.0.contains("no usable quote style"), "{}", err.0);
    }

    #[test]
    fn the_escaped_form_is_what_tier_three_writes() {
        assert_eq!(escaped("a\r\nb"), "a\\r\\nb");
        assert_eq!(escaped("plain"), "plain");
    }

    #[test]
    fn a_name_outside_the_alphabet_is_refused() {
        assert!(line("_ok9", "v").is_ok());
        assert!(line("1A", "v").is_err());
        assert!(line("A-B", "v").is_err());
    }
}
