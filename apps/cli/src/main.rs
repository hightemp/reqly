use std::io::IsTerminal;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let color = std::io::stdout().is_terminal();
    let mut out = std::io::stdout().lock();
    let mut err = std::io::stderr();
    let code = reqly_cli::run_cli(args, &mut out, &mut err, color).await;
    std::process::exit(code);
}
