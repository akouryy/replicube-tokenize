use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(about = "Tokenize Replicube Lua code and report size", version)]
struct Cli {
    code: String,
    #[arg(short, long, value_enum)]
    format: Format,
}

#[derive(Copy, Clone, ValueEnum)]
enum Format {
    Short,
    Long,
}

fn main() {
    let cli = Cli::parse();
    let (tokens, warnings) = replicube_tokenize::tokenize(&cli.code);
    if matches!(cli.format, Format::Long) {
        for tok in &tokens {
            match tok.cost() {
                Some(cost) => println!("{cost}\t{}", tok.text),
                None => println!("?\t{}", tok.text),
            }
        }
        println!("---");
    }
    let total: Option<usize> = tokens.iter().map(|tok| tok.cost()).sum();
    match total {
        Some(total) => println!("{total}"),
        None => println!("unknown"),
    }
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}
