//! Command-line arguments.

pub const USAGE: &str = "\
Usage:
  reqly-cli run <FILE> [OPTIONS]
  reqly-cli run --server <URL> --collection <ID|NAME> [OPTIONS]

Runs the requests of a collection with their tests and assertions and exits
with 0 when every check passed, 1 when one failed and 2 on other errors.

Source:
  <FILE>                     An export: Reqly, Insomnia, Postman, Bruno, HAR, OpenAPI…
      --server <URL>         A Reqly server; the collection is exported from it
      --token <TOKEN>        API token for --server (default: $REQLY_TOKEN)
      --collection <ID|NAME> The collection to run (needed when there are several)

Run:
  -e, --env <NAME>           Environment of the collection (default: its active one)
      --var <NAME=VALUE>     Set a variable, overriding every other value (repeatable)
  -d, --data <FILE>          CSV or JSON data file: one iteration per row
  -n, --iterations <N>       Iterations (default: one per data row, or one)
      --folder <NAME|PATH>   Run only this folder (`Parent/Child` for nested ones)
      --delay <MS>           Pause between requests
      --bail                 Stop at the first failed request

Output:
  -r, --reporter <NAME>      Console output: pretty (default), json or junit
      --report <FILE>        Also write a report; .html, .xml (JUnit) or .json (repeatable)
      --no-color             No colors in the pretty output

  -h, --help                 Show this help
  -V, --version              Show the version
";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reporter {
    #[default]
    Pretty,
    Json,
    Junit,
}

#[derive(Debug, Default)]
pub struct RunArgs {
    pub file: Option<String>,
    pub server: Option<String>,
    pub token: Option<String>,
    pub collection: Option<String>,
    pub env: Option<String>,
    pub vars: Vec<(String, String)>,
    pub data: Option<String>,
    pub iterations: Option<u32>,
    pub folder: Option<String>,
    pub delay_ms: Option<u64>,
    pub bail: bool,
    pub reporter: Reporter,
    pub reports: Vec<String>,
    pub no_color: bool,
}

#[derive(Debug)]
pub enum Command {
    Help,
    Version,
    Run(Box<RunArgs>),
}

pub fn parse(args: &[String]) -> Result<Command, String> {
    let mut it = args.iter();
    match it.next().map(String::as_str) {
        None => Err("no command".into()),
        Some("-h" | "--help" | "help") => Ok(Command::Help),
        Some("-V" | "--version") => Ok(Command::Version),
        Some("run") => parse_run(it).map(|r| Command::Run(Box::new(r))),
        Some(other) => Err(format!("unknown command `{other}`")),
    }
}

fn parse_run<'a>(mut it: impl Iterator<Item = &'a String>) -> Result<RunArgs, String> {
    let mut run = RunArgs::default();
    while let Some(arg) = it.next() {
        // `--name=value` is the same as `--name value`.
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "--server" => run.server = Some(value(flag)?),
            "--token" => run.token = Some(value(flag)?),
            "--collection" => run.collection = Some(value(flag)?),
            "-e" | "--env" => run.env = Some(value(flag)?),
            "--var" => {
                let v = value(flag)?;
                let (name, val) = v
                    .split_once('=')
                    .ok_or_else(|| format!("--var expects NAME=VALUE, got `{v}`"))?;
                run.vars.push((name.trim().to_string(), val.to_string()));
            }
            "-d" | "--data" => run.data = Some(value(flag)?),
            "-n" | "--iterations" => {
                let v = value(flag)?;
                run.iterations =
                    Some(v.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
                        format!("--iterations expects a positive number, got `{v}`")
                    })?);
            }
            "--folder" => run.folder = Some(value(flag)?),
            "--delay" => {
                let v = value(flag)?;
                run.delay_ms = Some(
                    v.parse()
                        .map_err(|_| format!("--delay expects milliseconds, got `{v}`"))?,
                );
            }
            "--bail" => run.bail = true,
            "-r" | "--reporter" => {
                run.reporter = match value(flag)?.as_str() {
                    "pretty" => Reporter::Pretty,
                    "json" => Reporter::Json,
                    "junit" => Reporter::Junit,
                    other => {
                        return Err(format!("unknown reporter `{other}` (pretty, json, junit)"));
                    }
                }
            }
            "--report" => run.reports.push(value(flag)?),
            "--no-color" => run.no_color = true,
            f if f.starts_with('-') && f.len() > 1 => return Err(format!("unknown option `{f}`")),
            _ if run.file.is_none() => run.file = Some(arg.clone()),
            _ => return Err(format!("unexpected argument `{arg}`")),
        }
    }
    match (&run.file, &run.server) {
        (None, None) => Err("give a file to run or --server".into()),
        (Some(_), Some(_)) => Err("give either a file or --server, not both".into()),
        (None, Some(_)) if run.collection.is_none() => Err("--server needs --collection".into()),
        _ => Ok(run),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_a_file_run() {
        let Command::Run(r) = parse(&args(
            "run c.json -e dev --var a=1 --var=b=x=y -d d.csv -n 2 --folder A/B --delay 5 --bail -r junit --report r.html",
        ))
        .unwrap() else {
            panic!()
        };
        assert_eq!(r.file.as_deref(), Some("c.json"));
        assert_eq!(r.env.as_deref(), Some("dev"));
        assert_eq!(
            r.vars,
            [("a".into(), "1".into()), ("b".into(), "x=y".into())]
        );
        assert_eq!(r.iterations, Some(2));
        assert_eq!(r.folder.as_deref(), Some("A/B"));
        assert_eq!(r.delay_ms, Some(5));
        assert!(r.bail);
        assert_eq!(r.reporter, Reporter::Junit);
        assert_eq!(r.reports, ["r.html"]);
    }

    #[test]
    fn refuses_bad_input() {
        for bad in [
            "run",
            "run a.json --server http://x",
            "run --server http://x",
            "run a.json -n 0",
            "run a.json --var novalue",
            "run a.json -r xml",
            "run a.json --what",
            "run a.json b.json",
            "fly",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
        assert!(parse(&args("run --server http://x --collection c")).is_ok());
    }
}
