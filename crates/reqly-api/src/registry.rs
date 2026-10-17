use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use reqly_core::{CoreError, CoreResult};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use ts_rs::TS;

/// A single typed operation exposed to the UI.
pub trait Command<C>: Send + Sync + 'static {
    /// Dotted name, e.g. `app.info`. Used as the Tauri command argument and
    /// in the server route `POST /api/v1/cmd/{name}`.
    const NAME: &'static str;
    type Input: DeserializeOwned + TS + Send + 'static;
    type Output: Serialize + TS + Send + 'static;

    fn run(ctx: &C, input: Self::Input) -> impl Future<Output = CoreResult<Self::Output>> + Send;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
type Handler<C> = Arc<dyn Fn(&C, Value) -> BoxFuture<'_, CoreResult<Value>> + Send + Sync>;

/// Descriptor of a registered command used for TypeScript generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSignature {
    pub name: &'static str,
    pub input: String,
    pub output: String,
}

pub struct Registry<C> {
    handlers: BTreeMap<&'static str, Handler<C>>,
    signatures: Vec<CommandSignature>,
    ts_decls: BTreeMap<String, String>,
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Self {
            handlers: BTreeMap::new(),
            signatures: Vec::new(),
            ts_decls: BTreeMap::new(),
        }
    }
}

impl<C: Send + Sync + 'static> Registry<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `K`. Panics on duplicate names: that is a programming error.
    pub fn register<K: Command<C>>(&mut self) -> &mut Self {
        assert!(
            !self.handlers.contains_key(K::NAME),
            "command {} registered twice",
            K::NAME
        );

        let handler: Handler<C> = Arc::new(|ctx, input| {
            Box::pin(async move {
                let input: K::Input = serde_json::from_value(input).map_err(|e| {
                    CoreError::new("command.invalid_input", e.to_string())
                        .with_param("name", K::NAME)
                })?;
                let output = K::run(ctx, input).await?;
                serde_json::to_value(output).map_err(|e| CoreError::internal(e.to_string()))
            })
        });
        self.handlers.insert(K::NAME, handler);

        let cfg = ts_rs::Config::new();
        self.signatures.push(CommandSignature {
            name: K::NAME,
            input: K::Input::name(&cfg),
            output: K::Output::name(&cfg),
        });
        for (name, path) in [
            (K::Input::name(&cfg), K::Input::output_path()),
            (K::Output::name(&cfg), K::Output::output_path()),
        ] {
            if let Some(path) = path {
                let stem = path.with_extension("").to_string_lossy().replace('\\', "/");
                self.ts_decls.insert(name, stem);
            }
        }
        self
    }

    pub async fn dispatch(&self, ctx: &C, name: &str, input: Value) -> CoreResult<Value> {
        let handler = self.handlers.get(name).ok_or_else(|| {
            CoreError::new("command.unknown", "unknown command").with_param("name", name)
        })?;
        handler(ctx, input).await
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.handlers.keys().copied()
    }

    pub fn signatures(&self) -> &[CommandSignature] {
        &self.signatures
    }

    /// Imports needed by the generated `commands.ts`: type name → file stem.
    pub fn ts_types(&self) -> &BTreeMap<String, String> {
        &self.ts_decls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    struct Ctx {
        greeting: &'static str,
    }

    #[derive(Deserialize, TS)]
    struct GreetInput {
        name: String,
    }

    #[derive(Serialize, TS)]
    struct GreetOutput {
        text: String,
    }

    struct Greet;

    impl Command<Ctx> for Greet {
        const NAME: &'static str = "test.greet";
        type Input = GreetInput;
        type Output = GreetOutput;

        async fn run(ctx: &Ctx, input: GreetInput) -> CoreResult<GreetOutput> {
            if input.name.is_empty() {
                return Err(CoreError::new("greet.empty_name", "name is empty"));
            }
            Ok(GreetOutput {
                text: format!("{}, {}", ctx.greeting, input.name),
            })
        }
    }

    fn registry() -> Registry<Ctx> {
        let mut r = Registry::new();
        r.register::<Greet>();
        r
    }

    const CTX: Ctx = Ctx { greeting: "Hello" };

    #[tokio::test]
    async fn dispatches_json_to_typed_handler() {
        let out = registry()
            .dispatch(&CTX, "test.greet", serde_json::json!({ "name": "Ann" }))
            .await
            .unwrap();
        assert_eq!(out, serde_json::json!({ "text": "Hello, Ann" }));
    }

    #[tokio::test]
    async fn unknown_command_is_an_error() {
        let err = registry()
            .dispatch(&CTX, "nope", Value::Null)
            .await
            .unwrap_err();
        assert_eq!(err.code, "command.unknown");
        assert_eq!(err.params["name"], "nope");
    }

    #[tokio::test]
    async fn invalid_input_is_an_error() {
        let err = registry()
            .dispatch(&CTX, "test.greet", serde_json::json!({ "nam": 1 }))
            .await
            .unwrap_err();
        assert_eq!(err.code, "command.invalid_input");
    }

    #[tokio::test]
    async fn handler_errors_pass_through() {
        let err = registry()
            .dispatch(&CTX, "test.greet", serde_json::json!({ "name": "" }))
            .await
            .unwrap_err();
        assert_eq!(err.code, "greet.empty_name");
    }

    #[test]
    fn records_signatures() {
        let r = registry();
        assert_eq!(r.names().collect::<Vec<_>>(), ["test.greet"]);
        assert_eq!(
            r.signatures(),
            [CommandSignature {
                name: "test.greet",
                input: "GreetInput".into(),
                output: "GreetOutput".into()
            }]
        );
    }

    #[test]
    #[should_panic(expected = "test.greet")]
    fn duplicate_registration_panics() {
        let mut r = registry();
        r.register::<Greet>();
    }
}
