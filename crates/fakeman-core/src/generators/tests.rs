use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};

use super::*;

fn ctx(seed: u64) -> GenContext {
    GenContext::seeded(seed).at(now())
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

fn gen_with(key: &str, params: Value, seed: u64) -> Result<Value, GeneratorError> {
    generate(&GeneratorRef::with_params(key, params), &mut ctx(seed))
}

const PLAN_KEYS: &[&str] = &[
    "person.firstName",
    "person.lastName",
    "person.fullName",
    "person.title",
    "person.jobTitle",
    "internet.email",
    "internet.safeEmail",
    "internet.username",
    "internet.password",
    "internet.url",
    "internet.domain",
    "internet.ipv4",
    "internet.ipv6",
    "internet.userAgent",
    "phone.phoneNumber",
    "location.street",
    "location.city",
    "location.zipCode",
    "location.state",
    "location.country",
    "location.countryCode",
    "location.latitude",
    "location.longitude",
    "company.name",
    "company.buzzword",
    "company.catchPhrase",
    "company.industry",
    "lorem.word",
    "lorem.words",
    "lorem.sentence",
    "lorem.paragraph",
    "number.int",
    "number.float",
    "datatype.boolean",
    "datatype.uuid",
    "date.past",
    "date.future",
    "date.between",
    "finance.creditCardNumber",
    "finance.currencyCode",
    "finance.amount",
    "static.string",
    "static.number",
    "static.boolean",
    "static.null",
];

#[test]
fn catalog_is_exactly_the_planned_keys() {
    let keys: Vec<&str> = catalog().iter().map(|def| def.key).collect();
    assert_eq!(keys, PLAN_KEYS);
}

#[test]
fn every_key_generates_its_declared_type() {
    for def in catalog() {
        for seed in 0..50 {
            let value = gen_with(def.key, json!({}), seed).unwrap();
            assert!(
                def.output.matches(&value),
                "{} produced {value}, expected {:?}",
                def.key,
                def.output
            );
        }
    }
}

#[test]
fn string_generators_are_not_empty_except_static() {
    for def in catalog()
        .iter()
        .filter(|def| def.output == ValueKind::String)
    {
        if def.key == "static.string" {
            continue;
        }
        for seed in 0..20 {
            let value = gen_with(def.key, json!({}), seed).unwrap();
            assert!(!value.as_str().unwrap().is_empty(), "{} was empty", def.key);
        }
    }
}

#[test]
fn every_param_default_passes_its_own_validation() {
    for def in catalog() {
        let defaults: Map<String, Value> = def
            .params
            .iter()
            .map(|param| (param.name.to_string(), param.kind.default_value()))
            .collect();
        def.resolve_params(&Value::Object(defaults))
            .unwrap_or_else(|err| panic!("{}: {err}", def.key));
        def.resolve_params(&Value::Null).unwrap();
    }
}

#[test]
fn keys_are_unique_and_namespaced() {
    let mut seen = std::collections::HashSet::new();
    for def in catalog() {
        assert!(seen.insert(def.key), "duplicate key {}", def.key);
        let (namespace, name) = def.key.split_once('.').expect("namespace.name");
        assert!(
            namespace.chars().all(|c| c.is_ascii_lowercase()),
            "{}",
            def.key
        );
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric()),
            "{}",
            def.key
        );
        assert!(!def.label.is_empty());
    }
}

#[test]
fn lookup_finds_keys_and_reports_unknown_ones() {
    assert_eq!(lookup("person.firstName").unwrap().label, "First name");
    assert_eq!(
        lookup("person.middleName").unwrap_err(),
        GeneratorError::UnknownKey("person.middleName".into())
    );
    assert_eq!(
        validate(&GeneratorRef::new("nope")),
        Err(GeneratorError::UnknownKey("nope".into()))
    );
}

#[test]
fn categories_group_the_whole_catalog_in_order() {
    let groups = categories();
    let names: Vec<&str> = groups.iter().map(|group| group.name).collect();
    assert_eq!(
        names,
        [
            "Person", "Internet", "Phone", "Location", "Company", "Lorem", "Number", "Datatype",
            "Date", "Finance", "Static"
        ]
    );
    let flattened: Vec<&str> = groups
        .iter()
        .flat_map(|group| group.generators.iter().map(|def| def.key))
        .collect();
    assert_eq!(flattened, PLAN_KEYS);
    for group in &groups {
        assert!(
            group
                .generators
                .iter()
                .all(|def| def.category == group.name)
        );
    }
}

#[test]
fn number_int_respects_range_and_validates_min_max() {
    for seed in 0..200 {
        let value = gen_with("number.int", json!({ "min": -3, "max": 3 }), seed).unwrap();
        assert!((-3..=3).contains(&value.as_i64().unwrap()));
    }
    assert_eq!(
        gen_with("number.int", json!({ "min": 7, "max": 7 }), 0).unwrap(),
        json!(7)
    );
    let full = json!({ "min": i64::MIN, "max": i64::MAX });
    assert!(gen_with("number.int", full, 0).unwrap().is_i64());
    assert_eq!(
        validate(&GeneratorRef::with_params(
            "number.int",
            json!({ "min": 5, "max": 1 })
        )),
        Err(GeneratorError::InvalidParam {
            key: "number.int".into(),
            param: "max",
            reason: "must not be less than min".into()
        })
    );
}

#[test]
fn params_are_type_checked() {
    let invalid = |key: &str, params: Value| match validate(&GeneratorRef::with_params(key, params))
    {
        Err(GeneratorError::InvalidParam { param, .. }) => param,
        other => panic!("{key}: expected InvalidParam, got {other:?}"),
    };
    assert_eq!(invalid("number.int", json!({ "min": "1" })), "min");
    assert_eq!(invalid("number.int", json!({ "max": 1.5 })), "max");
    assert_eq!(
        invalid("number.float", json!({ "decimals": 11 })),
        "decimals"
    );
    assert_eq!(
        invalid("number.float", json!({ "min": 2.5, "max": 1 })),
        "max"
    );
    assert_eq!(
        invalid("number.float", json!({ "min": -1e308, "max": 1e308 })),
        "max"
    );
    assert_eq!(
        invalid("internet.password", json!({ "minLength": 0 })),
        "minLength"
    );
    assert_eq!(
        invalid(
            "internet.password",
            json!({ "minLength": 20, "maxLength": 10 })
        ),
        "maxLength"
    );
    assert_eq!(invalid("lorem.words", json!({ "count": 0 })), "count");
    assert_eq!(invalid("date.past", json!({ "years": 0 })), "years");
    assert_eq!(
        invalid("date.between", json!({ "from": "yesterday" })),
        "from"
    );
    assert_eq!(
        invalid(
            "date.between",
            json!({ "from": "2025-01-02T00:00:00Z", "to": "2025-01-01T00:00:00Z" })
        ),
        "to"
    );
    assert_eq!(invalid("static.string", json!({ "value": 5 })), "value");
    assert_eq!(invalid("static.number", json!({ "value": "5" })), "value");
    assert_eq!(
        invalid("static.boolean", json!({ "value": "true" })),
        "value"
    );
    assert_eq!(
        validate(&GeneratorRef::with_params("number.int", json!([1, 2]))),
        Err(GeneratorError::ParamsNotObject {
            key: "number.int".into()
        })
    );
}

#[test]
fn unknown_and_null_params_fall_back_gracefully() {
    let value = gen_with("number.int", json!({ "min": null, "max": 3, "step": 2 }), 0).unwrap();
    assert!((0..=3).contains(&value.as_i64().unwrap()));
}

#[test]
fn number_float_and_amount_respect_range_and_decimals() {
    for key in ["number.float", "finance.amount"] {
        for seed in 0..200 {
            let value = gen_with(key, json!({ "min": 1.5, "max": 2.5, "decimals": 1 }), seed)
                .unwrap()
                .as_f64()
                .unwrap();
            assert!((1.5..=2.5).contains(&value), "{key}: {value}");
            assert_eq!((value * 10.0).round() / 10.0, value, "{key}: {value}");
        }
    }
}

#[test]
fn coordinates_stay_in_range() {
    for seed in 0..500 {
        let lat = gen_with("location.latitude", json!({}), seed).unwrap();
        let lng = gen_with("location.longitude", json!({}), seed).unwrap();
        assert!((-90.0..=90.0).contains(&lat.as_f64().unwrap()));
        assert!((-180.0..=180.0).contains(&lng.as_f64().unwrap()));
    }
}

#[test]
fn password_and_words_honour_their_counts() {
    for seed in 0..100 {
        let password = gen_with(
            "internet.password",
            json!({ "minLength": 5, "maxLength": 7 }),
            seed,
        )
        .unwrap();
        assert!((5..=7).contains(&password.as_str().unwrap().chars().count()));
        let words = gen_with("lorem.words", json!({ "count": 4 }), seed).unwrap();
        assert_eq!(words.as_str().unwrap().split(' ').count(), 4);
    }
}

#[test]
fn dates_are_iso_8601_and_on_the_right_side_of_now() {
    let parse = |value: Value| {
        DateTime::parse_from_rfc3339(value.as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc)
    };
    let year = chrono::Duration::days(365);
    for seed in 0..100 {
        let past = parse(gen_with("date.past", json!({}), seed).unwrap());
        assert!(past < now() && past >= now() - year);
        let future = parse(gen_with("date.future", json!({ "years": 2 }), seed).unwrap());
        assert!(future > now() && future <= now() + year * 2);
        let between = parse(
            gen_with(
                "date.between",
                json!({ "from": "2024-02-01T00:00:00+02:00", "to": "2024-02-02T00:00:00Z" }),
                seed,
            )
            .unwrap(),
        );
        assert!(between >= Utc.with_ymd_and_hms(2024, 1, 31, 22, 0, 0).unwrap());
        assert!(between <= Utc.with_ymd_and_hms(2024, 2, 2, 0, 0, 0).unwrap());
    }
    let text = gen_with("date.past", json!({}), 0).unwrap();
    assert!(text.as_str().unwrap().ends_with('Z'));
}

#[test]
fn uuid_is_a_valid_v4() {
    let value = gen_with("datatype.uuid", json!({}), 9).unwrap();
    let uuid = uuid::Uuid::parse_str(value.as_str().unwrap()).unwrap();
    assert_eq!(uuid.get_version_num(), 4);
}

#[test]
fn statics_return_their_value_with_its_type() {
    assert_eq!(
        gen_with("static.string", json!({ "value": "hi" }), 0).unwrap(),
        json!("hi")
    );
    assert_eq!(
        gen_with("static.number", json!({ "value": 42 }), 0).unwrap(),
        json!(42)
    );
    assert_eq!(
        gen_with("static.number", json!({ "value": 2.5 }), 0).unwrap(),
        json!(2.5)
    );
    assert_eq!(
        gen_with("static.number", json!({}), 0).unwrap().to_string(),
        "0"
    );
    assert_eq!(
        gen_with("static.boolean", json!({ "value": true }), 0).unwrap(),
        json!(true)
    );
    assert_eq!(gen_with("static.null", json!({}), 0).unwrap(), Value::Null);
}
