//! Built-in functions (template tags without side effects).

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use reqly_core::{CoreError, CoreResult};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::Digest;
use ts_rs::TS;

use crate::render::{Locale, value_to_string};

pub(crate) fn bad_arg(function: &str, message: impl Into<String>) -> CoreError {
    CoreError::new("template.bad_argument", message.into()).with_param("function", function)
}

pub(crate) struct Args<'a> {
    pub name: &'a str,
    pub values: &'a [Value],
}

impl Args<'_> {
    pub fn str(&self, i: usize) -> CoreResult<String> {
        self.values
            .get(i)
            .map(value_to_string)
            .ok_or_else(|| bad_arg(self.name, format!("argument {} is required", i + 1)))
    }

    pub fn str_or(&self, i: usize, default: &str) -> String {
        self.values
            .get(i)
            .map(value_to_string)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| default.to_string())
    }

    pub fn num_or(&self, i: usize, default: f64) -> CoreResult<f64> {
        match self.values.get(i) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Number(n)) => Ok(n.as_f64().unwrap_or(default)),
            Some(Value::String(s)) if s.trim().is_empty() => Ok(default),
            Some(Value::String(s)) => s
                .trim()
                .parse()
                .map_err(|_| bad_arg(self.name, format!("{s} is not a number"))),
            Some(other) => Err(bad_arg(self.name, format!("{other} is not a number"))),
        }
    }
}

/// Postman dynamic variables backed by faker-js functions.
const POSTMAN: &[(&str, &str)] = &[
    ("randomAbbreviation", "hacker.abbreviation"),
    ("randomAdjective", "hacker.adjective"),
    ("randomAvatarImage", "image.avatar"),
    ("randomBankAccount", "finance.accountNumber"),
    ("randomBankAccountBic", "finance.bic"),
    ("randomBankAccountIban", "finance.iban"),
    ("randomBankAccountName", "finance.accountName"),
    ("randomBitcoin", "finance.bitcoinAddress"),
    ("randomBoolean", "datatype.boolean"),
    ("randomBs", "company.buzzPhrase"),
    ("randomBsAdjective", "company.buzzAdjective"),
    ("randomBsBuzz", "company.buzzVerb"),
    ("randomBsNoun", "company.buzzNoun"),
    ("randomCatchPhrase", "company.catchPhrase"),
    ("randomCatchPhraseAdjective", "company.catchPhraseAdjective"),
    (
        "randomCatchPhraseDescriptor",
        "company.catchPhraseDescriptor",
    ),
    ("randomCatchPhraseNoun", "company.catchPhraseNoun"),
    ("randomCity", "location.city"),
    ("randomColor", "color.human"),
    ("randomCommonFileExt", "system.commonFileExt"),
    ("randomCommonFileName", "system.commonFileName"),
    ("randomCommonFileType", "system.commonFileType"),
    ("randomCompanyName", "company.name"),
    ("randomCountry", "location.country"),
    ("randomCountryCode", "location.countryCode"),
    ("randomCurrencyCode", "finance.currencyCode"),
    ("randomCurrencyName", "finance.currencyName"),
    ("randomCurrencySymbol", "finance.currencySymbol"),
    ("randomDatabaseCollation", "database.collation"),
    ("randomDatabaseColumn", "database.column"),
    ("randomDatabaseEngine", "database.engine"),
    ("randomDatabaseType", "database.type"),
    ("randomDateFuture", "date.future"),
    ("randomDatePast", "date.past"),
    ("randomDateRecent", "date.recent"),
    ("randomDepartment", "commerce.department"),
    ("randomDirectoryPath", "system.directoryPath"),
    ("randomDomainName", "internet.domainName"),
    ("randomDomainSuffix", "internet.domainSuffix"),
    ("randomDomainWord", "internet.domainWord"),
    ("randomEmail", "internet.email"),
    ("randomExampleEmail", "internet.exampleEmail"),
    ("randomFileExt", "system.fileExt"),
    ("randomFileName", "system.fileName"),
    ("randomFilePath", "system.filePath"),
    ("randomFileType", "system.fileType"),
    ("randomFirstName", "person.firstName"),
    ("randomFullName", "person.fullName"),
    ("randomHexColor", "color.rgb"),
    ("randomIP", "internet.ipv4"),
    ("randomIPV6", "internet.ipv6"),
    ("randomImageDataUri", "image.dataUri"),
    ("randomImageUrl", "image.url"),
    ("randomIngverb", "hacker.ingverb"),
    ("randomJobArea", "person.jobArea"),
    ("randomJobDescriptor", "person.jobDescriptor"),
    ("randomJobTitle", "person.jobTitle"),
    ("randomJobType", "person.jobType"),
    ("randomLastName", "person.lastName"),
    ("randomLatitude", "location.latitude"),
    ("randomLongitude", "location.longitude"),
    ("randomLoremLines", "lorem.lines"),
    ("randomLoremParagraph", "lorem.paragraph"),
    ("randomLoremParagraphs", "lorem.paragraphs"),
    ("randomLoremSentence", "lorem.sentence"),
    ("randomLoremSentences", "lorem.sentences"),
    ("randomLoremSlug", "lorem.slug"),
    ("randomLoremText", "lorem.text"),
    ("randomLoremWord", "lorem.word"),
    ("randomLoremWords", "lorem.words"),
    ("randomMACAddress", "internet.mac"),
    ("randomMimeType", "system.mimeType"),
    ("randomMonth", "date.month"),
    ("randomNamePrefix", "person.prefix"),
    ("randomNameSuffix", "person.suffix"),
    ("randomNoun", "hacker.noun"),
    ("randomPassword", "internet.password"),
    ("randomPhoneNumber", "phone.number"),
    ("randomPhoneNumberExt", "phone.number"),
    ("randomPhrase", "hacker.phrase"),
    ("randomPrice", "commerce.price"),
    ("randomProduct", "commerce.product"),
    ("randomProductAdjective", "commerce.productAdjective"),
    ("randomProductMaterial", "commerce.productMaterial"),
    ("randomProductName", "commerce.productName"),
    ("randomProtocol", "internet.protocol"),
    ("randomSemver", "system.semver"),
    ("randomStreetAddress", "location.streetAddress"),
    ("randomStreetName", "location.street"),
    ("randomTransactionType", "finance.transactionType"),
    ("randomUserAgent", "internet.userAgent"),
    ("randomUserName", "internet.username"),
    ("randomUrl", "internet.url"),
    ("randomVerb", "hacker.verb"),
    ("randomWeekday", "date.weekday"),
    ("randomWord", "lorem.word"),
    ("randomWords", "lorem.words"),
];

/// Postman image variables: `$randomCatsImage` is a LoremFlickr `cats` image.
const POSTMAN_IMAGES: &[(&str, &str)] = &[
    ("randomAbstractImage", "abstract"),
    ("randomAnimalsImage", "animals"),
    ("randomBusinessImage", "business"),
    ("randomCatsImage", "cats"),
    ("randomCityImage", "city"),
    ("randomFashionImage", "fashion"),
    ("randomFoodImage", "food"),
    ("randomNatureImage", "nature"),
    ("randomNightlifeImage", "nightlife"),
    ("randomPeopleImage", "people"),
    ("randomSportsImage", "sports"),
    ("randomTransportImage", "transport"),
];

/// Names of the Postman dynamic variables (without `$`).
pub fn dynamic_variables() -> Vec<&'static str> {
    let mut names: Vec<&str> = [
        "guid",
        "timestamp",
        "isoTimestamp",
        "randomUUID",
        "randomInt",
    ]
    .into_iter()
    .chain(["randomAlphaNumeric", "randomCreditCardMask"])
    .chain(POSTMAN.iter().map(|(n, _)| *n))
    .chain(POSTMAN_IMAGES.iter().map(|(n, _)| *n))
    .collect();
    names.sort_unstable();
    names
}

/// Postman-style dynamic variables and other short names.
pub fn resolve_alias(name: &str, args: Vec<Value>) -> (String, Vec<Value>) {
    let faker = |path: &str, rest: &[Value]| {
        let mut values = vec![json!(path)];
        values.extend_from_slice(rest);
        ("faker".to_string(), values)
    };
    match name {
        "guid" | "randomUUID" => ("uuid".into(), vec![json!("v4")]),
        "timestamp" => ("now".into(), vec![json!("unix")]),
        "isoTimestamp" => ("now".into(), vec![json!("iso-8601")]),
        "randomInt" => ("random".into(), vec![json!("int"), json!(0), json!(1000)]),
        "randomAlphaNumeric" => faker("string.alphanumeric", &[json!(1)]),
        "randomCreditCardMask" => faker("string.numeric", &[json!(4)]),
        _ => {
            if let Some((_, path)) = POSTMAN.iter().find(|(n, _)| *n == name) {
                faker(path, &[])
            } else if let Some((_, category)) = POSTMAN_IMAGES.iter().find(|(n, _)| *n == name) {
                faker("image.urlLoremFlickr", &[json!({ "category": category })])
            } else {
                (name.to_string(), args)
            }
        }
    }
}

/// Built-in functions; `None` when `name` is not one of them.
pub fn call(name: &str, values: &[Value], locale: Locale) -> Option<CoreResult<Value>> {
    let a = Args { name, values };
    Some(match name {
        "base64" => base64(&a),
        "urlencode" => urlencode(&a),
        "hash" => hash(&a),
        "hmac" => hmac(&a),
        "uuid" => uuid(&a),
        "now" => now(&a),
        "random" => random(&a),
        "jsonpath" => jsonpath(&a),
        "jwt" => jwt(&a),
        "faker" => {
            // Insomnia's faker tag uses Postman names: `{% faker 'randomFullName' %}`.
            let path = match a.str(0) {
                Ok(p) => p,
                Err(e) => return Some(Err(e)),
            };
            if !path.contains('.') {
                let (alias, args) = resolve_alias(&path, Vec::new());
                if alias != path {
                    return call(&alias, &args, locale);
                }
            }
            reqly_faker::call(locale, &path, a.values.get(1..).unwrap_or_default())
        }
        _ => return None,
    })
}

fn base64(a: &Args) -> CoreResult<Value> {
    // Insomnia order: action, kind (`normal` / `url`), value; the kind may be omitted.
    let action = a.str(0)?;
    let (url, value) = if a.values.len() >= 3 {
        (a.str(1)? == "url", a.str(2)?)
    } else {
        (false, a.str(1)?)
    };
    let out = match action.as_str() {
        "encode" if url => URL_SAFE_NO_PAD.encode(value),
        "encode" => STANDARD.encode(value),
        "decode" => {
            let trimmed = value.trim().trim_end_matches('=');
            let bytes = URL_SAFE_NO_PAD
                .decode(trimmed.replace('+', "-").replace('/', "_"))
                .map_err(|e| bad_arg(a.name, e.to_string()))?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        other => return Err(bad_arg(a.name, format!("unknown action {other}"))),
    };
    Ok(Value::String(out))
}

fn urlencode(a: &Args) -> CoreResult<Value> {
    use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
    const SET: &percent_encoding::AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let value = a.str(1)?;
    Ok(Value::String(match a.str(0)?.as_str() {
        "encode" => utf8_percent_encode(&value, SET).to_string(),
        "decode" => percent_decode_str(&value).decode_utf8_lossy().into_owned(),
        other => return Err(bad_arg(a.name, format!("unknown action {other}"))),
    }))
}

fn digest_bytes(a: &Args, algo: &str, data: &[u8]) -> CoreResult<Vec<u8>> {
    Ok(match algo {
        "md5" => md5::Md5::digest(data).to_vec(),
        "sha1" => sha1::Sha1::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "sha384" => sha2::Sha384::digest(data).to_vec(),
        "sha512" => sha2::Sha512::digest(data).to_vec(),
        other => return Err(bad_arg(a.name, format!("unsupported algorithm {other}"))),
    })
}

fn encode_bytes(a: &Args, encoding: &str, bytes: &[u8]) -> CoreResult<Value> {
    Ok(Value::String(match encoding {
        "hex" => hex::encode(bytes),
        "base64" => STANDARD.encode(bytes),
        "base64url" => URL_SAFE_NO_PAD.encode(bytes),
        other => return Err(bad_arg(a.name, format!("unsupported encoding {other}"))),
    }))
}

fn hash(a: &Args) -> CoreResult<Value> {
    let algo = a.str(0)?;
    let encoding = a.str_or(1, "hex");
    let bytes = digest_bytes(a, &algo, a.str(2)?.as_bytes())?;
    encode_bytes(a, &encoding, &bytes)
}

fn hmac(a: &Args) -> CoreResult<Value> {
    use hmac::{Hmac, KeyInit, Mac};
    let algo = a.str(0)?;
    let encoding = a.str_or(1, "hex");
    let key = a.str(2)?;
    let data = a.str(3)?;
    macro_rules! mac {
        ($h:ty) => {{
            let mut m = Hmac::<$h>::new_from_slice(key.as_bytes())
                .map_err(|e| bad_arg(a.name, e.to_string()))?;
            m.update(data.as_bytes());
            m.finalize().into_bytes().to_vec()
        }};
    }
    let bytes = match algo.as_str() {
        "md5" => mac!(md5::Md5),
        "sha1" => mac!(sha1::Sha1),
        "sha256" => mac!(sha2::Sha256),
        "sha384" => mac!(sha2::Sha384),
        "sha512" => mac!(sha2::Sha512),
        other => return Err(bad_arg(a.name, format!("unsupported algorithm {other}"))),
    };
    encode_bytes(a, &encoding, &bytes)
}

fn uuid(a: &Args) -> CoreResult<Value> {
    Ok(Value::String(match a.str_or(0, "v4").as_str() {
        "v4" | "4" => uuid::Uuid::new_v4().to_string(),
        "v7" | "7" => uuid::Uuid::now_v7().to_string(),
        other => return Err(bad_arg(a.name, format!("unsupported version {other}"))),
    }))
}

fn now(a: &Args) -> CoreResult<Value> {
    let now = chrono::Utc::now();
    Ok(match a.str_or(0, "iso-8601").as_str() {
        "iso-8601" | "iso" => {
            Value::String(now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        }
        "unix" => json!(now.timestamp()),
        "millis" => json!(now.timestamp_millis()),
        "rfc-2822" => Value::String(now.to_rfc2822()),
        "custom" => {
            let pattern = a.str_or(1, "%Y-%m-%d %H:%M:%S");
            use std::fmt::Write;
            let mut out = String::new();
            write!(out, "{}", now.format(&pattern))
                .map_err(|_| bad_arg(a.name, "invalid format"))?;
            Value::String(out)
        }
        other => return Err(bad_arg(a.name, format!("unknown format {other}"))),
    })
}

fn random(a: &Args) -> CoreResult<Value> {
    use rand::RngExt;
    let mut rng = rand::rng();
    Ok(match a.str_or(0, "int").as_str() {
        "int" => {
            let min = a.num_or(1, 0.0)? as i64;
            let max = a.num_or(2, 1000.0)? as i64;
            if min > max {
                return Err(bad_arg(a.name, "min is greater than max"));
            }
            json!(rng.random_range(min..=max))
        }
        "float" => {
            let min = a.num_or(1, 0.0)?;
            let max = a.num_or(2, 1.0)?;
            if min > max {
                return Err(bad_arg(a.name, "min is greater than max"));
            }
            json!(rng.random_range(min..=max))
        }
        "string" => {
            let len = a.num_or(1, 16.0)?.clamp(0.0, 10_000.0) as usize;
            let charset: Vec<char> = a
                .str_or(
                    2,
                    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                )
                .chars()
                .collect();
            Value::String(
                (0..len)
                    .map(|_| charset[rng.random_range(0..charset.len())])
                    .collect(),
            )
        }
        "regex" => {
            let pattern = a.str(1)?;
            let generator = rand_regex::Regex::compile(&pattern, 16)
                .map_err(|e| bad_arg(a.name, e.to_string()))?;
            Value::String(rng.sample::<String, _>(&generator))
        }
        other => return Err(bad_arg(a.name, format!("unknown kind {other}"))),
    })
}

fn jsonpath(a: &Args) -> CoreResult<Value> {
    let input = match a.values.first() {
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::String(s.clone())),
        Some(v) => v.clone(),
        None => return Err(bad_arg(a.name, "argument 1 is required")),
    };
    let path =
        serde_json_path::JsonPath::parse(&a.str(1)?).map_err(|e| bad_arg(a.name, e.to_string()))?;
    let nodes = path.query(&input).all();
    Ok(match nodes.as_slice() {
        [] => Value::Null,
        [one] => (*one).clone(),
        many => Value::Array(many.iter().map(|v| (*v).clone()).collect()),
    })
}

fn jwt(a: &Args) -> CoreResult<Value> {
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    match a.str(0)?.as_str() {
        "encode" => {
            let claims: Value = match a.values.get(1) {
                Some(Value::String(s)) => {
                    serde_json::from_str(s).map_err(|e| bad_arg(a.name, e.to_string()))?
                }
                Some(v) => v.clone(),
                None => return Err(bad_arg(a.name, "payload is required")),
            };
            let secret = a.str(2)?;
            let alg: Algorithm = a
                .str_or(3, "HS256")
                .parse()
                .map_err(|_| bad_arg(a.name, "unsupported algorithm"))?;
            let key = match alg {
                Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512 => {
                    EncodingKey::from_secret(secret.as_bytes())
                }
                Algorithm::RS256
                | Algorithm::RS384
                | Algorithm::RS512
                | Algorithm::PS256
                | Algorithm::PS384
                | Algorithm::PS512 => EncodingKey::from_rsa_pem(secret.as_bytes())
                    .map_err(|e| bad_arg(a.name, e.to_string()))?,
                Algorithm::ES256 | Algorithm::ES384 => EncodingKey::from_ec_pem(secret.as_bytes())
                    .map_err(|e| bad_arg(a.name, e.to_string()))?,
                Algorithm::EdDSA => EncodingKey::from_ed_pem(secret.as_bytes())
                    .map_err(|e| bad_arg(a.name, e.to_string()))?,
                #[allow(unreachable_patterns)]
                _ => return Err(bad_arg(a.name, "unsupported algorithm")),
            };
            let token = encode(&Header::new(alg), &claims, &key)
                .map_err(|e| bad_arg(a.name, e.to_string()))?;
            Ok(Value::String(token))
        }
        "decode" => {
            let token = a.str(1)?;
            let part = match a.str_or(2, "payload").as_str() {
                "header" => 0,
                "payload" => 1,
                other => return Err(bad_arg(a.name, format!("unknown part {other}"))),
            };
            let segment = token
                .split('.')
                .nth(part)
                .ok_or_else(|| bad_arg(a.name, "not a JWT"))?;
            let bytes = URL_SAFE_NO_PAD
                .decode(segment)
                .map_err(|e| bad_arg(a.name, e.to_string()))?;
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|e| bad_arg(a.name, e.to_string()))?;
            Ok(Value::String(value.to_string()))
        }
        other => Err(bad_arg(a.name, format!("unknown action {other}"))),
    }
}

/// Describes a function for autocompletion and the tag editor.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FunctionInfo {
    pub name: String,
    /// Argument names; `?` suffix marks optional ones.
    pub args: Vec<String>,
    /// Fixed choices for the first argument, if any.
    pub choices: Vec<String>,
    pub example: String,
}

pub fn catalog() -> Vec<FunctionInfo> {
    let f = |name: &str, args: &[&str], choices: &[&str], example: &str| FunctionInfo {
        name: name.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        choices: choices.iter().map(|s| s.to_string()).collect(),
        example: example.into(),
    };
    vec![
        f("uuid", &["version?"], &["v4", "v7"], "{% uuid 'v4' %}"),
        f(
            "now",
            &["format?", "pattern?"],
            &["iso-8601", "unix", "millis", "rfc-2822", "custom"],
            "{% now 'iso-8601' %}",
        ),
        f(
            "random",
            &["kind", "a?", "b?"],
            &["int", "float", "string", "regex"],
            "{% random 'int', 1, 100 %}",
        ),
        f(
            "base64",
            &["action", "kind", "value"],
            &["encode", "decode"],
            "{% base64 'encode', 'normal', 'text' %}",
        ),
        f(
            "urlencode",
            &["action", "value"],
            &["encode", "decode"],
            "{% urlencode 'encode', 'a b' %}",
        ),
        f(
            "hash",
            &["algorithm", "encoding", "value"],
            &["md5", "sha1", "sha256", "sha384", "sha512"],
            "{% hash 'sha256', 'hex', 'text' %}",
        ),
        f(
            "hmac",
            &["algorithm", "encoding", "key", "value"],
            &["md5", "sha1", "sha256", "sha384", "sha512"],
            "{% hmac 'sha256', 'hex', 'key', 'text' %}",
        ),
        f(
            "jsonpath",
            &["json", "path"],
            &[],
            "{% jsonpath data, '$.id' %}",
        ),
        f(
            "jwt",
            &["action", "payload", "secret?", "algorithm?"],
            &["encode", "decode"],
            "{% jwt 'encode', '{\"sub\":\"1\"}', 'secret', 'HS256' %}",
        ),
        f(
            "faker",
            &["function", "args?"],
            &[],
            "{{ $faker.person.firstName }}",
        ),
        f(
            "response",
            &["field", "requestId", "path?", "trigger?", "maxAgeSeconds?"],
            &["body", "header", "raw", "url", "status"],
            "{% response 'body', 'request-id', '$.token' %}",
        ),
        f(
            "cookie",
            &["url", "name"],
            &[],
            "{% cookie 'https://example.com', 'sid' %}",
        ),
        f(
            "request",
            &["field", "name?"],
            &["url", "method", "name", "header", "folder"],
            "{% request 'url' %}",
        ),
        f(
            "prompt",
            &["title", "label?", "default?", "key?"],
            &[],
            "{% prompt 'Token', 'Enter a token' %}",
        ),
        f("file", &["path"], &[], "{% file '/path/to/file.txt' %}"),
        f(
            "os",
            &["field"],
            &["hostname", "platform", "username", "env"],
            "{% os 'hostname' %}",
        ),
    ]
}
