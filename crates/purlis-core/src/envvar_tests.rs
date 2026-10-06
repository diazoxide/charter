use super::*;

fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| (*v).to_owned())
    }
}

fn os(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    pairs
        .iter()
        .map(|(n, v)| (OsString::from(n), OsString::from(v)))
        .collect()
}

#[test]
fn the_purlis_name_wins_when_both_are_set() {
    let env = env_of(&[("CHARTER_ROOT", "/old"), ("PURLIS_ROOT", "/new")]);
    assert_eq!(lookup("PURLIS_ROOT", &env).as_deref(), Some("/new"));
    // Asked by its old name, the answer is the same variable's.
    assert_eq!(lookup("CHARTER_ROOT", &env).as_deref(), Some("/new"));
}

#[test]
fn the_charter_name_is_read_when_the_purlis_one_is_absent() {
    let env = env_of(&[("CHARTER_HOOK_SOCKET", "/tmp/s.sock")]);
    assert_eq!(
        lookup("PURLIS_HOOK_SOCKET", &env).as_deref(),
        Some("/tmp/s.sock")
    );
    assert_eq!(lookup("PURLIS_CHAT", &env), None);
}

#[test]
fn a_purlis_name_set_to_nothing_still_wins() {
    let env = env_of(&[("CHARTER_HOME", "/old"), ("PURLIS_HOME", "")]);
    assert_eq!(lookup("PURLIS_HOME", &env).as_deref(), Some(""));
}

#[test]
fn a_variable_that_is_not_the_products_is_read_as_it_is() {
    let env = env_of(&[("HOME", "/home/me"), ("PURLIS_", "x")]);
    assert_eq!(lookup("HOME", &env).as_deref(), Some("/home/me"));
    assert_eq!(rest("PURLIS_"), None, "the bare prefix names nothing");
    assert_eq!(spellings("HOME"), ["HOME"]);
    assert_eq!(spellings("CHARTER_ROOT"), ["PURLIS_ROOT", "CHARTER_ROOT"]);
    assert!(same("CHARTER_CHAT", "PURLIS_CHAT"));
    assert!(!same("CHARTER_CHAT", "PURLIS_CHAT_TOKEN"));
}

#[test]
fn a_chat_is_given_each_product_variable_under_both_names() {
    let env = twinned(os(&[
        ("PATH", "/bin"),
        ("PURLIS_ROOT", "/plane"),
        ("CHARTER_HARNESS", "codex"),
    ]));
    assert_eq!(
        env,
        os(&[
            ("PATH", "/bin"),
            ("PURLIS_ROOT", "/plane"),
            ("CHARTER_ROOT", "/plane"),
            ("PURLIS_HARNESS", "codex"),
            ("CHARTER_HARNESS", "codex"),
        ])
    );
    assert_eq!(twinned(env.clone()), env, "twice is once");
}

#[test]
fn what_is_set_after_wins_under_both_names() {
    // Inherited first, set by the product after: the later value is the chat's, either name.
    let env = twinned(os(&[
        ("CHARTER_ROOT", "/inherited"),
        ("PURLIS_ROOT", "/set"),
    ]));
    assert_eq!(
        env,
        os(&[("PURLIS_ROOT", "/set"), ("CHARTER_ROOT", "/set")])
    );
}

#[test]
fn an_inherited_old_name_is_outranked_by_the_purlis_name_beside_it() {
    let env = os(&[
        ("CHARTER_ROOT", "/old"),
        ("PURLIS_ROOT", "/new"),
        ("CHARTER_HOME", "/h"),
    ]);
    let outranked = outranked(&env);
    assert!(outranked(OsStr::new("CHARTER_ROOT")));
    assert!(!outranked(OsStr::new("PURLIS_ROOT")));
    assert!(!outranked(OsStr::new("CHARTER_HOME")));
    assert!(!outranked(OsStr::new("PATH")));
}

fn os_env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
    move |name: &str| {
        pairs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| OsString::from(v))
    }
}

/// D-RN2d-8: a steering variable whose two names disagree is never silently picked from.
#[test]
fn a_steering_variable_whose_names_disagree_is_a_refusal_naming_both() {
    let found = disagreement_in(os_env(&[("PURLIS_ROOT", "/chat"), ("CHARTER_ROOT", "/x")]))
        .expect("a disagreement");
    assert_eq!(
        found.to_string(),
        "PURLIS_ROOT and CHARTER_ROOT disagree — set both to the same value (PURLIS_ROOT=<x> \
         CHARTER_ROOT=<x> charter …); CHARTER_ROOT is the old name."
    );
}

#[test]
fn equal_twins_and_an_old_name_alone_are_no_disagreement() {
    assert_eq!(
        disagreement_in(os_env(&[("PURLIS_ROOT", "/p"), ("CHARTER_ROOT", "/p")])),
        None
    );
    let alone = [("CHARTER_ROOT", "/old")];
    assert_eq!(disagreement_in(os_env(&alone)), None);
    assert_eq!(
        lookup("PURLIS_ROOT", os_env(&alone)),
        Some(OsString::from("/old"))
    );
}

#[test]
fn each_variable_that_chooses_what_is_acted_on_is_checked() {
    for rest in [
        "ROOT",
        "HOME",
        "WORKSPACE",
        "WORKTREES",
        "PLANE_ROOT_SESSION",
        "PERSONA",
        "CONFIG_HOME",
        "DATA_HOME",
    ] {
        let purlis = format!("PURLIS_{rest}");
        let charter = format!("CHARTER_{rest}");
        let pairs = [(purlis.as_str(), "a"), (charter.as_str(), "b")];
        assert!(disagreement_in(os_env(&pairs)).is_some(), "{rest}");
    }
}

#[test]
fn a_disagreeing_variable_that_chooses_nothing_is_read_under_the_purlis_name() {
    let pairs = [("PURLIS_LOG_DIR", "/new"), ("CHARTER_LOG_DIR", "/old")];
    assert_eq!(disagreement_in(os_env(&pairs)), None);
    assert_eq!(
        lookup("CHARTER_LOG_DIR", os_env(&pairs)),
        Some(OsString::from("/new"))
    );
}
