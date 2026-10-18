//! faker-js for Reqly (ADR-009).
//!
//! The vendored `@faker-js/faker` modules run in QuickJS. Templates call
//! [`call`], which runs on one long-lived runtime in a background thread;
//! scripts load the same modules into their own runtime through
//! [`Resolver`], [`Loader`] and [`import`]. Locales load on first use.

mod modules;
mod worker;

use std::fmt;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub use modules::{Loader, Resolver, import};

/// Web globals faker-js needs that QuickJS lacks (`TextEncoder`, `btoa`).
/// Evaluate in a context before using faker there.
pub const POLYFILLS: &str = include_str!("polyfills.js");
pub use worker::{call, warm_up};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocaleInfo {
    pub code: String,
    /// English name, e.g. `Russian`.
    pub title: String,
    /// Name in the language itself, e.g. `Русский`.
    pub endonym: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInfo {
    /// `module.method`, e.g. `person.firstName`.
    pub name: String,
    /// Parameters as declared, e.g. `options?: number | {min, max}`.
    pub args: String,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    version: String,
    locales: Vec<LocaleInfo>,
    functions: Vec<FunctionInfo>,
    exports: Vec<String>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../vendor/catalog.json")).expect("valid catalog.json")
    })
}

/// The vendored faker-js version.
pub fn version() -> &'static str {
    &catalog().version
}

pub fn locales() -> &'static [LocaleInfo] {
    &catalog().locales
}

pub fn functions() -> &'static [FunctionInfo] {
    &catalog().functions
}

/// Names exported by the `@faker-js/faker` package.
pub fn exports() -> &'static [String] {
    &catalog().exports
}

/// A faker-js locale code such as `en`, `ru` or `de_AT`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Locale(&'static str);

impl Locale {
    pub const EN: Locale = Locale("en");
    pub const RU: Locale = Locale("ru");

    /// `None` when faker-js has no such locale.
    pub fn parse(code: &str) -> Option<Locale> {
        locales()
            .iter()
            .find(|l| l.code == code)
            .map(|l| Locale(l.code.as_str()))
    }

    pub fn code(self) -> &'static str {
        self.0
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::EN
    }
}

impl fmt::Debug for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Serialize for Locale {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for Locale {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = String::deserialize(d)?;
        Locale::parse(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown faker locale {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_lists_locales_and_functions() {
        assert!(locales().len() > 50);
        assert!(Locale::parse("de_AT").is_some());
        assert!(Locale::parse("xx").is_none());
        assert!(functions().iter().any(|f| f.name == "person.firstName"));
        assert!(exports().iter().any(|e| e == "fakerRU"));
    }

    #[test]
    fn locale_serializes_as_code() {
        let l: Locale = serde_json::from_value(serde_json::json!("ru")).unwrap();
        assert_eq!(l, Locale::RU);
        assert_eq!(serde_json::to_value(l).unwrap(), serde_json::json!("ru"));
        assert!(serde_json::from_value::<Locale>(serde_json::json!("klingon")).is_err());
    }
}
