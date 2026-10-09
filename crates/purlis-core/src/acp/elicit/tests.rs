use serde_json::json;

use super::*;

fn request(value: serde_json::Value) -> CreateElicitationRequest {
    serde_json::from_value(value).expect("an elicitation request")
}

fn form_of(schema: serde_json::Value) -> CreateElicitationRequest {
    request(json!({
        "mode": "form",
        "sessionId": "s-1",
        "message": "Your registry token, please",
        "requestedSchema": schema,
    }))
}

fn token_form() -> Form {
    Form::read(
        &form_of(json!({
            "type": "object",
            "properties": {
                "token": {"type": "string", "title": "Token", "minLength": 4, "maxLength": 64},
                "region": {"type": "string", "oneOf": [
                    {"const": "eu", "title": "Europe"}, {"const": "us", "title": "America"}]},
                "tags": {"type": "array", "items": {"type": "string", "enum": ["a", "b"]},
                         "maxItems": 1},
                "port": {"type": "integer", "minimum": 1, "maximum": 65535},
                "ratio": {"type": "number", "minimum": 0, "maximum": 1},
                "save": {"type": "boolean"},
            },
            "required": ["token"],
        })),
        "s-1",
    )
    .expect("a form")
}

fn given(values: &[(&str, Given)]) -> BTreeMap<String, Given> {
    values
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

#[test]
fn a_form_for_this_session_is_read_with_each_field_its_kind() {
    let form = token_form();
    assert_eq!(form.message, "Your registry token, please");
    let token = form
        .fields
        .iter()
        .find(|f| f.name == "token")
        .expect("token");
    assert_eq!(token.label, "Token");
    assert!(token.required);
    assert_eq!(
        token.kind,
        Kind::Text {
            min: Some(4),
            max: Some(64),
            format: None
        }
    );
    let region = form
        .fields
        .iter()
        .find(|f| f.name == "region")
        .expect("region");
    assert_eq!(region.label, "region", "no title: its name");
    assert!(!region.required);
    assert_eq!(
        region.kind,
        Kind::One {
            choices: vec![
                Pick {
                    value: "eu".to_owned(),
                    label: "Europe".to_owned()
                },
                Pick {
                    value: "us".to_owned(),
                    label: "America".to_owned()
                },
            ]
        }
    );
}

#[test]
fn only_a_form_scoped_to_this_session_is_read() {
    let url = request(json!({
        "mode": "url", "sessionId": "s-1", "elicitationId": "e-1",
        "url": "https://example.invalid/login", "message": "Log in",
    }));
    assert_eq!(Form::read(&url, "s-1"), Err(Unfit::NotAForm));
    let schema = json!({"type": "object", "properties": {}});
    assert_eq!(
        Form::read(&form_of(schema.clone()), "s-2"),
        Err(Unfit::NotOurs)
    );
    let by_request = request(json!({
        "mode": "form", "requestId": 7, "message": "m", "requestedSchema": schema,
    }));
    assert_eq!(Form::read(&by_request, "s-1"), Err(Unfit::NotOurs));
}

#[test]
fn a_form_past_a_bound_or_with_a_field_purlis_cannot_draw_is_not_read() {
    let many: serde_json::Map<String, serde_json::Value> = (0..=MOST_FIELDS)
        .map(|at| (format!("f{at}"), json!({"type": "boolean"})))
        .collect();
    assert_eq!(
        Form::read(&form_of(json!({"properties": many})), "s-1"),
        Err(Unfit::TooMuch)
    );
    let choices: Vec<String> = (0..=MOST_CHOICES).map(|at| at.to_string()).collect();
    assert_eq!(
        Form::read(
            &form_of(json!({"properties": {"pick": {"type": "string", "enum": choices}}})),
            "s-1"
        ),
        Err(Unfit::TooMuch)
    );
    let long_name = "n".repeat(MOST_NAME_BYTES + 1);
    assert_eq!(
        Form::read(
            &form_of(json!({"properties": {long_name: {"type": "boolean"}}})),
            "s-1"
        ),
        Err(Unfit::TooMuch)
    );
    assert_eq!(
        Form::read(
            &form_of(json!({"properties": {"odd": {"type": "object"}}})),
            "s-1"
        ),
        Err(Unfit::TooMuch)
    );
}

#[test]
fn what_the_agent_wrote_is_shown_as_plain_bounded_text() {
    let form = Form::read(
        &request(json!({
            "mode": "form", "sessionId": "s-1",
            "message": format!("line one\n\u{202e}evil\u{0007}\r\n{}", "m".repeat(MOST_MESSAGE_BYTES)),
            "requestedSchema": {"properties": {"f": {
                "type": "string",
                "title": format!("Ti\u{200b}tle\nnext {}", "t".repeat(MOST_LABEL_BYTES)),
                "description": "help\u{1b}[31m red",
            }}},
        })),
        "s-1",
    )
    .expect("a form");
    assert!(
        form.message.starts_with("line one\nevil\nmmm"),
        "{:?}",
        &form.message[..20]
    );
    assert!(form.message.len() <= MOST_MESSAGE_BYTES);
    assert!(form.message.ends_with('…'));
    let field = &form.fields[0];
    assert!(field.label.starts_with("Title next ttt"), "{}", field.label);
    assert!(field.label.len() <= MOST_LABEL_BYTES);
    assert_eq!(field.help, "help[31m red");
    assert_eq!(field.name, "f");
}

#[test]
fn a_unicode_line_separator_never_starts_a_line_in_a_label() {
    assert_eq!(
        plain("Pass\u{2028}word\u{2029}x", MOST_LABEL_BYTES, false),
        "Pass word x"
    );
    assert_eq!(
        plain("one\u{2028}two\u{2029}three", MOST_MESSAGE_BYTES, true),
        "one\ntwo\nthree"
    );
}

#[test]
fn an_elicitation_is_raised_as_an_ask_only_the_window_answers_with_no_deadline() {
    let ask = token_form().ask("s-1");
    assert!(ask.elicits_secret);
    assert_eq!(ask.deadline, Deadline::None);
    assert_eq!(
        ask.options
            .iter()
            .map(|o| o.id.as_str())
            .collect::<Vec<_>>(),
        [ACCEPT, DECLINE, CANCEL]
    );
    assert_eq!(ask.summary.as_str(), "Your registry token, please");
    assert!(
        ask.channel.request().is_none(),
        "never supersedes another ask"
    );
    assert!(matches!(ask.action, Action::Elicit { ref fields } if fields.len() == 6));
}

#[test]
fn values_that_fit_the_form_are_what_the_agent_is_sent() {
    let content = token_form()
        .check(&given(&[
            ("token", Given::Text("abcd1234".to_owned())),
            ("region", Given::Text("eu".to_owned())),
            ("tags", Given::Many(vec!["b".to_owned()])),
            ("port", Given::Integer(8080)),
            ("ratio", Given::Integer(1)),
            ("save", Given::Boolean(true)),
        ]))
        .expect("fits");
    assert_eq!(
        serde_json::to_value(&content).expect("JSON"),
        json!({"token": "abcd1234", "region": "eu", "tags": ["b"], "port": 8080,
               "ratio": 1.0, "save": true})
    );
}

#[test]
fn values_that_do_not_fit_are_refused_by_field_and_never_echoed() {
    let form = token_form();
    let secret = "s3cr3t-value-never-echoed";
    let refusals = [
        given(&[]),
        given(&[("token", Given::Text("abc".to_owned()))]),
        given(&[("token", Given::Integer(5))]),
        given(&[
            ("token", Given::Text("abcd".to_owned())),
            ("region", Given::Text(secret.to_owned())),
        ]),
        given(&[
            ("token", Given::Text("abcd".to_owned())),
            ("tags", Given::Many(vec!["a".to_owned(), "b".to_owned()])),
        ]),
        given(&[
            ("token", Given::Text("abcd".to_owned())),
            ("port", Given::Integer(0)),
        ]),
        given(&[
            ("token", Given::Text("abcd".to_owned())),
            ("ratio", Given::Number(1.5)),
        ]),
        given(&[
            ("token", Given::Text(secret.to_owned())),
            (secret, Given::Text(secret.to_owned())),
        ]),
    ];
    for values in &refusals {
        let why = form.check(values).expect_err("refused");
        assert!(!why.contains("s3cr3t-value"), "{why}");
    }
    assert_eq!(
        form.check(&given(&[("token", Given::Text("abc".to_owned()))])),
        Err("Token is not of a length it takes".to_owned())
    );
}

#[test]
fn an_answer_never_prints_its_values() {
    let answer = Elicited::Accept(given(&[("token", Given::Text("hunter2".to_owned()))]));
    let printed = format!("{answer:?}");
    assert!(!printed.contains("hunter2"), "{printed}");
    assert!(printed.contains("token"), "{printed}");
}

#[test]
fn the_window_s_values_read_as_what_they_are() {
    let values: BTreeMap<String, Given> = serde_json::from_value(json!({
        "a": "text", "b": 3, "c": 0.5, "d": false, "e": ["x"],
    }))
    .expect("values");
    assert!(matches!(values["a"], Given::Text(_)));
    assert!(matches!(values["b"], Given::Integer(3)));
    assert!(matches!(values["c"], Given::Number(_)));
    assert!(matches!(values["d"], Given::Boolean(false)));
    assert!(matches!(values["e"], Given::Many(_)));
}
