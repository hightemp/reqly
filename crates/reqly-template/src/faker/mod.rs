//! Faker with faker-js naming (`person.firstName`, `internet.email`, …) and
//! built-in English and Russian data. The `fake` crate has no Russian locale
//! (ADR-005), so data lives here.

mod en;
mod ru;

use rand::RngExt;
use reqly_core::{CoreError, CoreResult};
use serde_json::{Value, json};

use crate::functions::{Args, bad_arg};
use crate::render::Locale;

fn pick<'a>(list: &'a [&'a str]) -> &'a str {
    list[rand::rng().random_range(0..list.len())]
}

fn digits(n: usize) -> String {
    let mut rng = rand::rng();
    (0..n)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

fn chars(n: usize, set: &str) -> String {
    let set: Vec<char> = set.chars().collect();
    let mut rng = rand::rng();
    (0..n)
        .map(|_| set[rng.random_range(0..set.len())])
        .collect()
}

struct Person {
    first: String,
    last: String,
    middle: String,
}

fn person(locale: Locale) -> Person {
    let female = rand::rng().random_bool(0.5);
    match locale {
        Locale::En => Person {
            first: pick(if female {
                en::FIRST_FEMALE
            } else {
                en::FIRST_MALE
            })
            .into(),
            last: pick(en::LAST).into(),
            middle: pick(if female {
                en::FIRST_FEMALE
            } else {
                en::FIRST_MALE
            })
            .into(),
        },
        Locale::Ru => {
            let last = pick(ru::LAST);
            Person {
                first: pick(if female {
                    ru::FIRST_FEMALE
                } else {
                    ru::FIRST_MALE
                })
                .into(),
                last: if female {
                    ru::feminine(last)
                } else {
                    last.to_string()
                },
                middle: pick(if female {
                    ru::PATRONYMIC_FEMALE
                } else {
                    ru::PATRONYMIC_MALE
                })
                .into(),
            }
        }
    }
}

fn latin(locale: Locale, s: &str) -> String {
    match locale {
        Locale::En => s.to_lowercase(),
        Locale::Ru => ru::translit(s),
    }
}

fn words(locale: Locale, n: usize) -> Vec<&'static str> {
    (0..n)
        .map(|_| {
            pick(if locale == Locale::Ru {
                ru::WORDS
            } else {
                en::WORDS
            })
        })
        .collect()
}

fn sentence(locale: Locale) -> String {
    let n = rand::rng().random_range(5..12);
    let mut s = words(locale, n).join(" ");
    if let Some(first) = s.chars().next() {
        s = first.to_uppercase().collect::<String>() + &s[first.len_utf8()..];
    }
    s + "."
}

fn iso_days_from_now(days: i64) -> String {
    let offset = chrono::Duration::seconds(rand::rng().random_range(0..86_400))
        + chrono::Duration::days(days);
    (chrono::Utc::now() + offset).to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn call(a: &Args, locale: Locale) -> CoreResult<Value> {
    let path = a.str(0)?;
    let rest = Args {
        name: a.name,
        values: a.values.get(1..).unwrap_or_default(),
    };
    let l = locale;
    let ru = l == Locale::Ru;
    let pick_l = |en_list: &'static [&'static str], ru_list: &'static [&'static str]| {
        pick(if ru { ru_list } else { en_list }).to_string()
    };
    let s = |v: String| Ok(Value::String(v));
    let mut rng = rand::rng();

    match path.as_str() {
        "person.firstName" => s(person(l).first),
        "person.lastName" => s(person(l).last),
        "person.middleName" => s(person(l).middle),
        "person.fullName" => {
            let p = person(l);
            s(if ru {
                format!("{} {} {}", p.last, p.first, p.middle)
            } else {
                format!("{} {}", p.first, p.last)
            })
        }
        "person.prefix" => s(if ru {
            String::new()
        } else {
            pick(en::PREFIX).into()
        }),
        "person.sex" | "person.gender" => s(if ru {
            ["мужской", "женский"]
        } else {
            ["male", "female"]
        }[rng.random_range(0..2)]
        .into()),
        "person.jobTitle" => s(pick_l(en::JOBS, ru::JOBS)),

        "internet.username" | "internet.userName" => {
            let p = person(l);
            s(format!(
                "{}{}{}",
                latin(l, &p.first),
                ["", ".", "_"][rng.random_range(0..3)],
                digits(rng.random_range(0..4))
            ))
        }
        "internet.email" => {
            let p = person(l);
            s(format!(
                "{}.{}{}@{}",
                latin(l, &p.first),
                latin(l, &p.last),
                digits(rng.random_range(0..3)),
                pick(if ru { ru::DOMAINS } else { en::DOMAINS })
            ))
        }
        "internet.domainName" => s(format!(
            "{}.{}",
            latin(l, &pick_l(en::WORDS, ru::WORDS)),
            ["com", "net", "org", "io", "dev"][rng.random_range(0..5)]
        )),
        "internet.url" => s(format!(
            "https://{}.{}/",
            latin(l, &pick_l(en::WORDS, ru::WORDS)),
            ["com", "net", "org", "io"][rng.random_range(0..4)]
        )),
        "internet.ipv4" | "internet.ip" => s(format!(
            "{}.{}.{}.{}",
            rng.random_range(1..=223),
            rng.random::<u8>(),
            rng.random::<u8>(),
            rng.random_range(1..=254)
        )),
        "internet.ipv6" => s((0..8)
            .map(|_| format!("{:x}", rng.random::<u16>()))
            .collect::<Vec<_>>()
            .join(":")),
        "internet.mac" => s((0..6)
            .map(|_| format!("{:02x}", rng.random::<u8>()))
            .collect::<Vec<_>>()
            .join(":")),
        "internet.port" => Ok(json!(rng.random_range(1024..=65535))),
        "internet.password" => {
            let len = rest.num_or(0, 16.0)?.clamp(4.0, 256.0) as usize;
            s(chars(
                len,
                "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789!@#$%^&*",
            ))
        }
        "internet.userAgent" => s(format!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36",
            rng.random_range(110..140)
        )),

        "location.city" => s(pick_l(en::CITIES, ru::CITIES)),
        "location.street" => s(pick_l(en::STREETS, ru::STREETS)),
        "location.streetAddress" => s(if ru {
            format!("{}, д. {}", pick(ru::STREETS), rng.random_range(1..200))
        } else {
            format!("{} {}", rng.random_range(1..9999), pick(en::STREETS))
        }),
        "location.state" => s(pick_l(en::STATES, ru::STATES)),
        "location.country" => s(pick_l(en::COUNTRIES, ru::COUNTRIES)),
        "location.zipCode" => s(digits(if ru { 6 } else { 5 })),
        "location.latitude" => Ok(json!(
            (rng.random_range(-90.0..=90.0f64) * 1e6).round() / 1e6
        )),
        "location.longitude" => Ok(json!(
            (rng.random_range(-180.0..=180.0f64) * 1e6).round() / 1e6
        )),

        "phone.number" => s(if ru {
            format!(
                "+7 9{} {}-{}-{}",
                digits(2),
                digits(3),
                digits(2),
                digits(2)
            )
        } else {
            format!("+1 ({}) {}-{}", digits(3), digits(3), digits(4))
        }),

        "company.name" => s(if ru {
            format!("{} «{}»", pick(ru::COMPANY_PREFIX), pick(ru::COMPANY_WORDS))
        } else {
            format!("{} {}", pick(en::LAST), pick(en::COMPANY_SUFFIX))
        }),
        "company.catchPhrase" => s(format!(
            "{} {}",
            pick_l(en::CATCH, ru::CATCH),
            pick_l(en::CATCH_NOUN, ru::CATCH_NOUN)
        )),

        "lorem.word" => s(words(l, 1).join("")),
        "lorem.words" => s(words(l, rest.num_or(0, 3.0)?.clamp(1.0, 1000.0) as usize).join(" ")),
        "lorem.sentence" => s(sentence(l)),
        "lorem.sentences" => s((0..rest.num_or(0, 3.0)?.clamp(1.0, 100.0) as usize)
            .map(|_| sentence(l))
            .collect::<Vec<_>>()
            .join(" ")),
        "lorem.paragraph" => s((0..rng.random_range(3..6))
            .map(|_| sentence(l))
            .collect::<Vec<_>>()
            .join(" ")),
        "lorem.slug" => s(words(l, 3)
            .iter()
            .map(|w| latin(l, w))
            .collect::<Vec<_>>()
            .join("-")),

        "number.int" => {
            let min = rest.num_or(0, 0.0)? as i64;
            let max = rest.num_or(1, 1_000_000.0)? as i64;
            if min > max {
                return Err(bad_arg(a.name, "min is greater than max"));
            }
            Ok(json!(rng.random_range(min..=max)))
        }
        "number.float" => {
            let min = rest.num_or(0, 0.0)?;
            let max = rest.num_or(1, 1.0)?;
            let precision = rest.num_or(2, 2.0)?.clamp(0.0, 10.0) as i32;
            if min > max {
                return Err(bad_arg(a.name, "min is greater than max"));
            }
            let f = 10f64.powi(precision);
            Ok(json!((rng.random_range(min..=max) * f).round() / f))
        }
        "string.uuid" => s(uuid::Uuid::new_v4().to_string()),
        "string.alpha" => s(chars(
            rest.num_or(0, 8.0)?.clamp(1.0, 10_000.0) as usize,
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        )),
        "string.alphanumeric" => s(chars(
            rest.num_or(0, 8.0)?.clamp(1.0, 10_000.0) as usize,
            "abcdefghijklmnopqrstuvwxyz0123456789",
        )),
        "string.numeric" => s(digits(rest.num_or(0, 6.0)?.clamp(1.0, 10_000.0) as usize)),
        "datatype.boolean" => Ok(json!(rng.random_bool(0.5))),

        "date.past" => s(iso_days_from_now(-rng.random_range(1..365))),
        "date.future" => s(iso_days_from_now(rng.random_range(1..365))),
        "date.recent" => s(iso_days_from_now(-rng.random_range(0..2))),
        "date.soon" => s(iso_days_from_now(rng.random_range(0..2))),
        "date.birthdate" => s(iso_days_from_now(-rng.random_range(18 * 365..80 * 365))
            .split('T')
            .next()
            .unwrap_or_default()
            .to_string()),
        "date.anytime" => s(iso_days_from_now(rng.random_range(-3650..3650))),
        "date.month" => s(pick_l(en::MONTHS, ru::MONTHS)),
        "date.weekday" => s(pick_l(en::WEEKDAYS, ru::WEEKDAYS)),

        "color.human" => s(pick_l(en::COLORS, ru::COLORS)),
        "color.rgb" => s(format!(
            "#{:02x}{:02x}{:02x}",
            rng.random::<u8>(),
            rng.random::<u8>(),
            rng.random::<u8>()
        )),

        "commerce.product" => s(pick_l(en::PRODUCTS, ru::PRODUCTS)),
        "commerce.productName" => s(format!(
            "{} {}",
            pick_l(en::ADJECTIVES, ru::ADJECTIVES),
            pick_l(en::PRODUCTS, ru::PRODUCTS).to_lowercase()
        )),
        "commerce.price" => s(format!("{:.2}", rng.random_range(1.0..1000.0f64))),
        "commerce.department" => s(pick_l(en::DEPARTMENTS, ru::DEPARTMENTS)),

        "finance.amount" => s(format!("{:.2}", rng.random_range(0.0..10_000.0f64))),
        "finance.accountNumber" => s(digits(if ru { 20 } else { 10 })),
        "finance.iban" => s(if ru {
            format!("RU{}{}", digits(2), digits(29))
        } else {
            format!("GB{}NWBK{}", digits(2), digits(14))
        }),
        "finance.currencyCode" => {
            s(["USD", "EUR", "RUB", "GBP", "JPY", "CNY", "KZT"][rng.random_range(0..7)].into())
        }
        "finance.creditCardNumber" => s(format!(
            "4{} {} {} {}",
            digits(3),
            digits(4),
            digits(4),
            digits(4)
        )),
        "finance.bic" => s(if ru {
            format!("04{}", digits(7))
        } else {
            chars(8, "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
        }),

        "image.url" => s(format!(
            "https://picsum.photos/seed/{}/640/480",
            chars(8, "abcdefghijklmnopqrstuvwxyz0123456789")
        )),
        "image.avatar" => s(format!(
            "https://i.pravatar.cc/150?u={}",
            uuid::Uuid::new_v4()
        )),

        _ => Err(CoreError::new(
            "template.faker_unknown",
            format!("unknown faker function {path}"),
        )
        .with_param("name", path)),
    }
}

/// Faker function names with argument hints, for autocompletion.
pub const FAKER_FUNCTIONS: &[(&str, &str)] = &[
    ("person.firstName", ""),
    ("person.lastName", ""),
    ("person.middleName", ""),
    ("person.fullName", ""),
    ("person.prefix", ""),
    ("person.sex", ""),
    ("person.jobTitle", ""),
    ("internet.email", ""),
    ("internet.username", ""),
    ("internet.password", "length?"),
    ("internet.url", ""),
    ("internet.domainName", ""),
    ("internet.ipv4", ""),
    ("internet.ipv6", ""),
    ("internet.mac", ""),
    ("internet.port", ""),
    ("internet.userAgent", ""),
    ("location.city", ""),
    ("location.street", ""),
    ("location.streetAddress", ""),
    ("location.state", ""),
    ("location.country", ""),
    ("location.zipCode", ""),
    ("location.latitude", ""),
    ("location.longitude", ""),
    ("phone.number", ""),
    ("company.name", ""),
    ("company.catchPhrase", ""),
    ("lorem.word", ""),
    ("lorem.words", "count?"),
    ("lorem.sentence", ""),
    ("lorem.sentences", "count?"),
    ("lorem.paragraph", ""),
    ("lorem.slug", ""),
    ("number.int", "min?, max?"),
    ("number.float", "min?, max?, precision?"),
    ("string.uuid", ""),
    ("string.alpha", "length?"),
    ("string.alphanumeric", "length?"),
    ("string.numeric", "length?"),
    ("datatype.boolean", ""),
    ("date.past", ""),
    ("date.future", ""),
    ("date.recent", ""),
    ("date.soon", ""),
    ("date.birthdate", ""),
    ("date.anytime", ""),
    ("date.month", ""),
    ("date.weekday", ""),
    ("color.human", ""),
    ("color.rgb", ""),
    ("commerce.product", ""),
    ("commerce.productName", ""),
    ("commerce.price", ""),
    ("commerce.department", ""),
    ("finance.amount", ""),
    ("finance.accountNumber", ""),
    ("finance.iban", ""),
    ("finance.currencyCode", ""),
    ("finance.creditCardNumber", ""),
    ("finance.bic", ""),
    ("image.url", ""),
    ("image.avatar", ""),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_function_works_in_both_locales() {
        for (name, _) in FAKER_FUNCTIONS {
            for locale in [Locale::En, Locale::Ru] {
                let values = [json!(name)];
                let a = Args {
                    name: "faker",
                    values: &values,
                };
                let v = call(&a, locale).unwrap_or_else(|e| panic!("{name}: {e}"));
                if *name != "person.prefix" || locale == Locale::En {
                    assert!(
                        !crate::render::value_to_string(&v).is_empty(),
                        "{name} {locale:?}"
                    );
                }
            }
        }
    }
}
