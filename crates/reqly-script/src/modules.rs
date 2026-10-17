//! Bundled libraries, compiled to QuickJS bytecode once per process and
//! loaded into a script's context on first use (ADR-006).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rquickjs::module::{Evaluated, WriteOptions};
use rquickjs::{CatchResultExt, Ctx, Module, Promise, Runtime, Value};

const LIBRARIES: &[(&str, &str)] = &[
    ("chai", include_str!("../vendor/chai.js")),
    ("lodash", include_str!("../vendor/lodash.js")),
    ("crypto-js", include_str!("../vendor/crypto-js.js")),
    ("moment", include_str!("../vendor/moment.js")),
];

/// Runs a UMD/CommonJS bundle with `module`/`exports` and exports the result.
fn wrap(source: &str) -> String {
    format!(
        "const module = {{ exports: {{}} }};\nconst exports = module.exports;\n\
         (function (module, exports, define) {{\n{source}\n}}).call(globalThis, module, exports, undefined);\n\
         export default module.exports;"
    )
}

fn cache() -> &'static Mutex<HashMap<&'static str, Vec<u8>>> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Vec<u8>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Bytecode of a library, compiled on first use in a scratch runtime.
fn bytecode(name: &str) -> rquickjs::Result<Option<Vec<u8>>> {
    let Some((key, source)) = LIBRARIES.iter().find(|(n, _)| *n == name) else {
        return Ok(None);
    };
    if let Some(b) = cache().lock().ok().and_then(|c| c.get(key).cloned()) {
        return Ok(Some(b));
    }
    let rt = Runtime::new()?;
    let ctx = rquickjs::Context::full(&rt)?;
    let bytes =
        ctx.with(|ctx| Module::declare(ctx, *key, wrap(source))?.write(WriteOptions::default()))?;
    if let Ok(mut c) = cache().lock() {
        c.insert(key, bytes.clone());
    }
    Ok(Some(bytes))
}

/// The exports of a bundled library (`undefined` for unknown names).
#[allow(unsafe_code)]
pub fn load<'js>(ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Value<'js>> {
    let Some(bytes) = bytecode(name)? else {
        return Ok(Value::new_undefined(ctx.clone()));
    };
    // SAFETY: the bytecode was produced by this process's QuickJS from our own sources.
    let module = unsafe { Module::load(ctx.clone(), &bytes) }?;
    let (module, promise): (Module<'js, Evaluated>, Promise<'js>) = module.eval()?;
    promise
        .finish::<()>()
        .catch(ctx)
        .map_err(|e| rquickjs::Error::new_from_js_message("module", "exports", e.to_string()))?;
    module.get("default")
}
