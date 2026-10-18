//! Module loading: the vendored files compiled to QuickJS bytecode once per
//! process and shared by every runtime that loads them.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rquickjs::loader::ImportAttributes;
use rquickjs::module::{Declared, WriteOptions};
use rquickjs::{CatchResultExt, Ctx, Error, Module, Object, Result};

include!(concat!(env!("OUT_DIR"), "/files.rs"));

fn file(name: &str) -> Option<(&'static str, &'static str)> {
    FILES
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| FILES[i])
}

type Bytecode = Mutex<HashMap<&'static str, Arc<Vec<u8>>>>;

fn cache() -> &'static Bytecode {
    static CACHE: OnceLock<Bytecode> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// `dir/./a/../b.js` → `dir/b.js`.
fn join(base: &str, name: &str) -> String {
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for part in name.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// Resolves relative imports between the vendored files.
pub struct Resolver;

impl rquickjs::loader::Resolver for Resolver {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<String> {
        let path = if name.starts_with("./") || name.starts_with("../") {
            join(base, name)
        } else {
            name.to_string()
        };
        match file(&path) {
            Some(_) => Ok(path),
            None => Err(Error::new_resolving(base, name)),
        }
    }
}

/// Loads the vendored files by their resolved name.
pub struct Loader;

impl rquickjs::loader::Loader for Loader {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> Result<Module<'js, Declared>> {
        declare(ctx, name)
    }
}

#[allow(unsafe_code)]
fn declare<'js>(ctx: &Ctx<'js>, name: &str) -> Result<Module<'js, Declared>> {
    let (key, source) = file(name).ok_or_else(|| Error::new_loading(name))?;
    let cached = cache().lock().ok().and_then(|c| c.get(key).cloned());
    if let Some(bytes) = cached {
        // SAFETY: the bytecode was produced by this process's QuickJS from our own sources.
        return unsafe { Module::load(ctx.clone(), &bytes) };
    }
    let module = Module::declare(ctx.clone(), key, source)?;
    let bytes = module.write(WriteOptions::default())?;
    if let Ok(mut c) = cache().lock() {
        c.insert(key, Arc::new(bytes));
    }
    Ok(module)
}

/// Evaluates a vendored module (e.g. `faker/locale/ru.js`) and returns its
/// namespace. The runtime must use [`Resolver`] and [`Loader`] for its
/// imports. Each module must be imported once per context: keep the result.
pub fn import<'js>(ctx: &Ctx<'js>, name: &str) -> Result<Object<'js>> {
    let (module, promise) = declare(ctx, name)?.eval()?;
    promise
        .finish::<()>()
        .catch(ctx)
        .map_err(|e| Error::new_loading_message(name, e.to_string()))?;
    module.namespace()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_relative_paths() {
        assert_eq!(join("faker/locale/ru.js", "../ru-x.js"), "faker/ru-x.js");
        assert_eq!(
            join("faker/index.js", "./locale/de.js"),
            "faker/locale/de.js"
        );
    }

    #[test]
    fn files_are_sorted_for_lookup() {
        assert!(FILES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(file("faker/locale/ru.js").is_some());
        assert!(file("faker/index.js").is_some());
    }
}
