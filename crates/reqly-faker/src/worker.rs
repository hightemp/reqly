//! The template runtime: one QuickJS runtime on a background thread that
//! keeps the loaded locales between calls.

use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use reqly_core::{CoreError, CoreResult};
use rquickjs::prelude::Func;
use rquickjs::{CatchResultExt, Context, Ctx, Function, Runtime};
use serde_json::Value;

use crate::Locale;
use crate::modules::{Loader, Resolver, import};

/// A single call may run this long (loading a locale included).
const TIME_LIMIT: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 5 });
const MEMORY_LIMIT: usize = 256 * 1024 * 1024;

struct Job {
    locale: Locale,
    path: String,
    args: String,
    reply: Option<SyncSender<CoreResult<Value>>>,
}

fn worker() -> &'static Mutex<Option<Sender<Job>>> {
    static WORKER: OnceLock<Mutex<Option<Sender<Job>>>> = OnceLock::new();
    WORKER.get_or_init(Default::default)
}

fn spawn() -> Sender<Job> {
    let (tx, rx) = mpsc::channel();
    // QuickJS recurses on the native stack.
    let _ = std::thread::Builder::new()
        .name("reqly-faker".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || run(rx));
    tx
}

fn submit(mut job: Job) -> bool {
    let Ok(mut w) = worker().lock() else {
        return false;
    };
    for _ in 0..2 {
        let tx = w.get_or_insert_with(spawn);
        match tx.send(job) {
            Ok(()) => return true,
            // The thread is gone: start a new one and retry once.
            Err(mpsc::SendError(back)) => {
                job = back;
                *w = None;
            }
        }
    }
    false
}

fn failed(path: &str, message: impl Into<String>) -> CoreError {
    let message = message.into();
    CoreError::new("template.faker_failed", format!("{path}: {message}"))
        .with_param("name", path)
        .with_param("message", message)
}

/// Calls the faker-js function `path` (`module.method`) in `locale` with
/// JSON arguments. Dates come back as ISO strings, big integers as strings.
pub fn call(locale: Locale, path: &str, args: &[Value]) -> CoreResult<Value> {
    let (tx, rx) = mpsc::sync_channel(1);
    let job = Job {
        locale,
        path: path.to_string(),
        args: Value::Array(args.to_vec()).to_string(),
        reply: Some(tx),
    };
    if !submit(job) {
        return Err(failed(path, "the faker runtime is not available"));
    }
    rx.recv()
        .unwrap_or_else(|_| Err(failed(path, "the faker runtime stopped")))
}

/// Loads `locale` in the background so that the first call is fast.
pub fn warm_up(locale: Locale) {
    submit(Job {
        locale,
        path: String::new(),
        args: "[]".into(),
        reply: None,
    });
}

fn run(rx: Receiver<Job>) {
    let mut engine: Option<Engine> = None;
    for job in rx {
        if engine.is_none() {
            engine = Engine::new().ok();
        }
        let result = match &engine {
            Some(e) => e.call(&job),
            None => Err(failed(&job.path, "cannot start the faker runtime")),
        };
        // A runaway call may leave the runtime unusable: start over.
        if matches!(&result, Err(e) if e.code == "template.faker_timeout"
            || e.message.contains("out of memory"))
        {
            engine = None;
        }
        if let Some(reply) = job.reply {
            let _ = reply.send(result);
        }
    }
}

fn install<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<()> {
    ctx.globals().set(
        "__import",
        Func::from(|ctx: Ctx<'js>, name: String| import(&ctx, &name)),
    )?;
    ctx.eval::<(), _>(crate::POLYFILLS)?;
    ctx.eval::<(), _>(include_str!("call.js"))
}

struct Engine {
    // Dropped before the runtime.
    ctx: Context,
    _rt: Runtime,
    deadline: Arc<Mutex<Option<Instant>>>,
}

impl Engine {
    fn new() -> rquickjs::Result<Self> {
        let rt = Runtime::new()?;
        rt.set_memory_limit(MEMORY_LIMIT);
        rt.set_loader(Resolver, Loader);
        let deadline: Arc<Mutex<Option<Instant>>> = Arc::default();
        let check = deadline.clone();
        rt.set_interrupt_handler(Some(Box::new(move || {
            check
                .lock()
                .ok()
                .and_then(|d| *d)
                .is_some_and(|d| Instant::now() > d)
        })));
        let ctx = Context::full(&rt)?;
        ctx.with(|ctx| install(&ctx))?;
        Ok(Self {
            ctx,
            _rt: rt,
            deadline,
        })
    }

    fn call(&self, job: &Job) -> CoreResult<Value> {
        if let Ok(mut d) = self.deadline.lock() {
            *d = Some(Instant::now() + TIME_LIMIT);
        }
        let out = self.ctx.with(|ctx| {
            let f: Function = ctx
                .globals()
                .get("__call")
                .catch(&ctx)
                .map_err(|e| e.to_string())?;
            f.call::<_, String>((job.locale.code(), job.path.as_str(), job.args.as_str()))
                .catch(&ctx)
                .map_err(|e| e.to_string())
        });
        let expired = self
            .deadline
            .lock()
            .ok()
            .and_then(|mut d| d.take())
            .is_some_and(|d| Instant::now() > d);
        let path = job.path.as_str();
        let text = match out {
            Ok(text) => text,
            Err(_) if expired => {
                return Err(CoreError::new(
                    "template.faker_timeout",
                    format!("{path} ran longer than {} s", TIME_LIMIT.as_secs()),
                )
                .with_param("name", path));
            }
            // The message without the JS stack.
            Err(message) => {
                let message = message.split("\n    at ").next().unwrap_or_default();
                return Err(failed(path, message.trim().trim_start_matches("Error: ")));
            }
        };
        let mut out: Value =
            serde_json::from_str(&text).map_err(|e| failed(path, e.to_string()))?;
        if out.get("unknown").is_some() {
            return Err(CoreError::new(
                "template.faker_unknown",
                format!("unknown faker function {path}"),
            )
            .with_param("name", path));
        }
        Ok(out.get_mut("value").map(Value::take).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn calls_faker_functions_in_a_locale() {
        let name = call(Locale::RU, "person.firstName", &[]).unwrap();
        let name = name.as_str().unwrap();
        assert!(
            name.chars()
                .all(|c| matches!(c, 'А'..='я' | 'ё' | 'Ё' | '-')),
            "{name}"
        );
        let de = Locale::parse("de_AT").unwrap();
        assert!(call(de, "location.city", &[]).unwrap().is_string());
    }

    #[test]
    fn passes_arguments_and_positional_shortcuts() {
        let n = call(Locale::EN, "number.int", &[json!({"min": 5, "max": 5})]).unwrap();
        assert_eq!(n, json!(5));
        let n = call(Locale::EN, "number.int", &[json!(7), json!(7)]).unwrap();
        assert_eq!(n, json!(7));
        let pick = call(Locale::EN, "helpers.arrayElement", &[json!(["only"])]).unwrap();
        assert_eq!(pick, json!("only"));
        let pw = call(Locale::EN, "internet.password", &[json!(12)]).unwrap();
        assert_eq!(pw.as_str().unwrap().len(), 12);
        let fake = call(Locale::EN, "helpers.fake", &[json!("{{number.int(3)}}!")]).unwrap();
        assert!(fake.as_str().unwrap().ends_with('!'));
    }

    #[test]
    fn converts_dates_and_big_integers() {
        let d = call(Locale::EN, "date.past", &[]).unwrap();
        assert!(d.as_str().unwrap().ends_with('Z'), "{d}");
        let b = call(Locale::EN, "number.bigInt", &[json!(1), json!(1)]).unwrap();
        assert_eq!(b, json!("1"));
    }

    #[test]
    fn rejects_unknown_and_internal_names() {
        for path in [
            "person.nope",
            "person.constructor",
            "person.toString",
            "fakerCore.randomizer",
            "rawDefinitions.person",
            "person",
            "a.b.c",
        ] {
            let e = call(Locale::EN, path, &[]).unwrap_err();
            assert_eq!(e.code, "template.faker_unknown", "{path}");
        }
    }

    #[test]
    fn reports_faker_errors() {
        let e = call(Locale::EN, "number.int", &[json!({"min": 10, "max": 1})]).unwrap_err();
        assert_eq!(e.code, "template.faker_failed");
    }

    #[test]
    fn stops_runaway_calls_and_recovers() {
        let e = call(Locale::EN, "lorem.words", &[json!(1_000_000_000)]).unwrap_err();
        assert_eq!(e.code, "template.faker_timeout", "{e:?}");
        assert!(call(Locale::EN, "person.firstName", &[]).is_ok());
    }

    #[test]
    fn catalog_functions_without_required_parameters_work() {
        for f in crate::functions() {
            let required = !f.args.is_empty() && !f.args.split(", ").next().unwrap().contains('?');
            let r = call(Locale::EN, &f.name, &[]);
            if !required {
                assert!(r.is_ok(), "{}: {r:?}", f.name);
            } else if let Err(e) = r {
                assert_ne!(e.code, "template.faker_unknown", "{}", f.name);
            }
        }
    }
}
