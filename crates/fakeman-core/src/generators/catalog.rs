//! The initial catalog (PLAN 3.2), backed by the `fake` crate.

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use fake::faker::address::en::{
    CityName, CountryCode, CountryName, StateName, StreetName, ZipCode,
};
use fake::faker::company::en::{Buzzword, CatchPhrase, CompanyName, Industry};
use fake::faker::creditcard::en::CreditCardNumber;
use fake::faker::currency::en::CurrencyCode;
use fake::faker::internet::en::{
    DomainSuffix, FreeEmail, IPv4, IPv6, Password, SafeEmail, UserAgent, Username,
};
use fake::faker::job::en::Title as JobTitle;
use fake::faker::lorem::en::{Paragraph, Sentence, Word, Words};
use fake::faker::name::en::{FirstName, LastName, Name, Title};
use fake::faker::phone_number::en::PhoneNumber;
use fake::{Dummy, Fake};
use rand::RngExt;
use serde_json::Value;

use super::{GeneratorDef, ParamDef, ParamIssue, ParamKind, Params, ValueKind};
use crate::rng::GenContext;

const PERSON: &str = "Person";
const INTERNET: &str = "Internet";
const PHONE: &str = "Phone";
const LOCATION: &str = "Location";
const COMPANY: &str = "Company";
const LOREM: &str = "Lorem";
const NUMBER: &str = "Number";
const DATATYPE: &str = "Datatype";
const DATE: &str = "Date";
const FINANCE: &str = "Finance";
const STATIC: &str = "Static";

const fn simple(
    key: &'static str,
    category: &'static str,
    label: &'static str,
    output: ValueKind,
    generate: fn(&Params, &mut GenContext) -> Value,
) -> GeneratorDef {
    GeneratorDef {
        key,
        category,
        label,
        output,
        params: &[],
        check: None,
        generate,
    }
}

const fn text(
    key: &'static str,
    category: &'static str,
    label: &'static str,
    generate: fn(&Params, &mut GenContext) -> Value,
) -> GeneratorDef {
    simple(key, category, label, ValueKind::String, generate)
}

fn fake_text<F>(faker: F, ctx: &mut GenContext) -> Value
where
    String: Dummy<F>,
{
    Value::String(faker.fake_with_rng(&mut ctx.rng))
}

fn domain(ctx: &mut GenContext) -> String {
    let word: String = Word().fake_with_rng(&mut ctx.rng);
    let suffix: String = DomainSuffix().fake_with_rng(&mut ctx.rng);
    format!("{}.{suffix}", word.to_lowercase())
}

fn decimal(min: f64, max: f64, decimals: i64, ctx: &mut GenContext) -> Value {
    let value = ctx.rng.random_range(min..=max);
    let factor = 10f64.powi(decimals as i32);
    let rounded = (value * factor).round() / factor;
    let rounded = if rounded.is_finite() { rounded } else { value };
    Value::from(rounded.clamp(min, max))
}

fn coordinate(limit: f64, ctx: &mut GenContext) -> Value {
    decimal(-limit, limit, 6, ctx)
}

fn iso(dt: DateTime<Utc>) -> Value {
    Value::String(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn year_offset(params: &Params, ctx: &mut GenContext) -> Duration {
    let span = params.int("years") * 365 * 24 * 60 * 60;
    Duration::seconds(ctx.rng.random_range(1..=span))
}

fn min_le_max(params: &Params) -> Result<(), ParamIssue> {
    let (min, max) = (params.float("min"), params.float("max"));
    if min > max {
        return Err(ParamIssue {
            param: "max",
            reason: "must not be less than min".into(),
        });
    }
    if !(max - min).is_finite() {
        return Err(ParamIssue {
            param: "max",
            reason: "is too far from min".into(),
        });
    }
    Ok(())
}

const INT_MIN_MAX: &[ParamDef] = &[
    ParamDef {
        name: "min",
        label: "Minimum",
        kind: ParamKind::Int {
            default: 0,
            min: i64::MIN,
            max: i64::MAX,
        },
    },
    ParamDef {
        name: "max",
        label: "Maximum",
        kind: ParamKind::Int {
            default: 1000,
            min: i64::MIN,
            max: i64::MAX,
        },
    },
];

const DECIMAL_PARAMS: &[ParamDef] = &[
    ParamDef {
        name: "min",
        label: "Minimum",
        kind: ParamKind::Number { default: 0.0 },
    },
    ParamDef {
        name: "max",
        label: "Maximum",
        kind: ParamKind::Number { default: 1000.0 },
    },
    ParamDef {
        name: "decimals",
        label: "Decimal places",
        kind: ParamKind::Int {
            default: 2,
            min: 0,
            max: 10,
        },
    },
];

const YEARS: &[ParamDef] = &[ParamDef {
    name: "years",
    label: "Within years",
    kind: ParamKind::Int {
        default: 1,
        min: 1,
        max: 100,
    },
}];

pub static CATALOG: &[GeneratorDef] = &[
    text("person.firstName", PERSON, "First name", |_, c| {
        fake_text(FirstName(), c)
    }),
    text("person.lastName", PERSON, "Last name", |_, c| {
        fake_text(LastName(), c)
    }),
    text("person.fullName", PERSON, "Full name", |_, c| {
        fake_text(Name(), c)
    }),
    text("person.title", PERSON, "Title (Mr., Dr.)", |_, c| {
        fake_text(Title(), c)
    }),
    text("person.jobTitle", PERSON, "Job title", |_, c| {
        fake_text(JobTitle(), c)
    }),
    text("internet.email", INTERNET, "Email", |_, c| {
        fake_text(FreeEmail(), c)
    }),
    text(
        "internet.safeEmail",
        INTERNET,
        "Safe email (example domains)",
        |_, c| fake_text(SafeEmail(), c),
    ),
    text("internet.username", INTERNET, "Username", |_, c| {
        fake_text(Username(), c)
    }),
    GeneratorDef {
        key: "internet.password",
        category: INTERNET,
        label: "Password",
        output: ValueKind::String,
        params: &[
            ParamDef {
                name: "minLength",
                label: "Minimum length",
                kind: ParamKind::Int {
                    default: 8,
                    min: 1,
                    max: 128,
                },
            },
            ParamDef {
                name: "maxLength",
                label: "Maximum length",
                kind: ParamKind::Int {
                    default: 16,
                    min: 1,
                    max: 128,
                },
            },
        ],
        check: Some(|p| {
            if p.int("minLength") > p.int("maxLength") {
                return Err(ParamIssue {
                    param: "maxLength",
                    reason: "must not be less than minLength".into(),
                });
            }
            Ok(())
        }),
        generate: |p, c| {
            let (min, max) = (p.int("minLength") as usize, p.int("maxLength") as usize);
            fake_text(Password(min..max + 1), c)
        },
    },
    text("internet.url", INTERNET, "URL", |_, c| {
        Value::String(format!("https://{}/", domain(c)))
    }),
    text("internet.domain", INTERNET, "Domain name", |_, c| {
        Value::String(domain(c))
    }),
    text("internet.ipv4", INTERNET, "IPv4 address", |_, c| {
        fake_text(IPv4(), c)
    }),
    text("internet.ipv6", INTERNET, "IPv6 address", |_, c| {
        fake_text(IPv6(), c)
    }),
    text("internet.userAgent", INTERNET, "User agent", |_, c| {
        fake_text(UserAgent(), c)
    }),
    text("phone.phoneNumber", PHONE, "Phone number", |_, c| {
        fake_text(PhoneNumber(), c)
    }),
    text("location.street", LOCATION, "Street", |_, c| {
        fake_text(StreetName(), c)
    }),
    text("location.city", LOCATION, "City", |_, c| {
        fake_text(CityName(), c)
    }),
    text("location.zipCode", LOCATION, "ZIP code", |_, c| {
        fake_text(ZipCode(), c)
    }),
    text("location.state", LOCATION, "State", |_, c| {
        fake_text(StateName(), c)
    }),
    text("location.country", LOCATION, "Country", |_, c| {
        fake_text(CountryName(), c)
    }),
    text("location.countryCode", LOCATION, "Country code", |_, c| {
        fake_text(CountryCode(), c)
    }),
    simple(
        "location.latitude",
        LOCATION,
        "Latitude",
        ValueKind::Number,
        |_, c| coordinate(90.0, c),
    ),
    simple(
        "location.longitude",
        LOCATION,
        "Longitude",
        ValueKind::Number,
        |_, c| coordinate(180.0, c),
    ),
    text("company.name", COMPANY, "Company name", |_, c| {
        fake_text(CompanyName(), c)
    }),
    text("company.buzzword", COMPANY, "Buzzword", |_, c| {
        fake_text(Buzzword(), c)
    }),
    text("company.catchPhrase", COMPANY, "Catch phrase", |_, c| {
        fake_text(CatchPhrase(), c)
    }),
    text("company.industry", COMPANY, "Industry", |_, c| {
        fake_text(Industry(), c)
    }),
    text("lorem.word", LOREM, "Word", |_, c| fake_text(Word(), c)),
    GeneratorDef {
        key: "lorem.words",
        category: LOREM,
        label: "Words",
        output: ValueKind::String,
        params: &[ParamDef {
            name: "count",
            label: "Number of words",
            kind: ParamKind::Int {
                default: 3,
                min: 1,
                max: 100,
            },
        }],
        check: None,
        generate: |p, c| {
            let count = p.int("count") as usize;
            let words: Vec<String> = Words(count..count + 1).fake_with_rng(&mut c.rng);
            Value::String(words.join(" "))
        },
    },
    text("lorem.sentence", LOREM, "Sentence", |_, c| {
        fake_text(Sentence(4..10), c)
    }),
    text("lorem.paragraph", LOREM, "Paragraph", |_, c| {
        fake_text(Paragraph(3..7), c)
    }),
    GeneratorDef {
        key: "number.int",
        category: NUMBER,
        label: "Integer",
        output: ValueKind::Integer,
        params: INT_MIN_MAX,
        check: Some(|p| {
            if p.int("min") > p.int("max") {
                return Err(ParamIssue {
                    param: "max",
                    reason: "must not be less than min".into(),
                });
            }
            Ok(())
        }),
        generate: |p, c| Value::from(c.rng.random_range(p.int("min")..=p.int("max"))),
    },
    GeneratorDef {
        key: "number.float",
        category: NUMBER,
        label: "Decimal number",
        output: ValueKind::Number,
        params: DECIMAL_PARAMS,
        check: Some(min_le_max),
        generate: |p, c| decimal(p.float("min"), p.float("max"), p.int("decimals"), c),
    },
    simple(
        "datatype.boolean",
        DATATYPE,
        "Boolean",
        ValueKind::Boolean,
        |_, c| Value::Bool(c.rng.random()),
    ),
    text("datatype.uuid", DATATYPE, "UUID", |_, c| {
        Value::String(
            uuid::Builder::from_random_bytes(c.rng.random())
                .into_uuid()
                .to_string(),
        )
    }),
    GeneratorDef {
        key: "date.past",
        category: DATE,
        label: "Date in the past",
        output: ValueKind::String,
        params: YEARS,
        check: None,
        generate: |p, c| iso(c.now - year_offset(p, c)),
    },
    GeneratorDef {
        key: "date.future",
        category: DATE,
        label: "Date in the future",
        output: ValueKind::String,
        params: YEARS,
        check: None,
        generate: |p, c| iso(c.now + year_offset(p, c)),
    },
    GeneratorDef {
        key: "date.between",
        category: DATE,
        label: "Date between",
        output: ValueKind::String,
        params: &[
            ParamDef {
                name: "from",
                label: "From",
                kind: ParamKind::DateTime {
                    default: "2020-01-01T00:00:00Z",
                },
            },
            ParamDef {
                name: "to",
                label: "To",
                kind: ParamKind::DateTime {
                    default: "2030-12-31T23:59:59Z",
                },
            },
        ],
        check: Some(|p| {
            if p.datetime("from") > p.datetime("to") {
                return Err(ParamIssue {
                    param: "to",
                    reason: "must not be before from".into(),
                });
            }
            Ok(())
        }),
        generate: |p, c| {
            let (from, to) = (p.datetime("from").timestamp(), p.datetime("to").timestamp());
            let seconds = c.rng.random_range(from..=to);
            iso(DateTime::from_timestamp(seconds, 0).unwrap_or_default())
        },
    },
    text(
        "finance.creditCardNumber",
        FINANCE,
        "Credit card number",
        |_, c| fake_text(CreditCardNumber(), c),
    ),
    text("finance.currencyCode", FINANCE, "Currency code", |_, c| {
        fake_text(CurrencyCode(), c)
    }),
    GeneratorDef {
        key: "finance.amount",
        category: FINANCE,
        label: "Amount",
        output: ValueKind::Number,
        params: DECIMAL_PARAMS,
        check: Some(min_le_max),
        generate: |p, c| decimal(p.float("min"), p.float("max"), p.int("decimals"), c),
    },
    GeneratorDef {
        key: "static.string",
        category: STATIC,
        label: "Fixed text",
        output: ValueKind::String,
        params: &[ParamDef {
            name: "value",
            label: "Value",
            kind: ParamKind::String { default: "" },
        }],
        check: None,
        generate: |p, _| p.get("value").clone(),
    },
    GeneratorDef {
        key: "static.number",
        category: STATIC,
        label: "Fixed number",
        output: ValueKind::Number,
        params: &[ParamDef {
            name: "value",
            label: "Value",
            kind: ParamKind::Number { default: 0.0 },
        }],
        check: None,
        generate: |p, _| p.get("value").clone(),
    },
    GeneratorDef {
        key: "static.boolean",
        category: STATIC,
        label: "Fixed boolean",
        output: ValueKind::Boolean,
        params: &[ParamDef {
            name: "value",
            label: "Value",
            kind: ParamKind::Bool { default: false },
        }],
        check: None,
        generate: |p, _| p.get("value").clone(),
    },
    simple("static.null", STATIC, "Null", ValueKind::Null, |_, _| {
        Value::Null
    }),
];
