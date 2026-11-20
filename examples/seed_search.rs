//! Read a starting 5×5 from JSON and print Dryland seeds that match it.
//!
//! ```text
//! cargo run --example seed_search -- examples/normal-dryland-9.json 0 64
//! ```
//!
//! The last two arguments are a half-open seed interval. There is no screenshot parsing.

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().unwrap_or_else(|| "examples/normal-dryland-9.json".to_string());
    let start = args
        .next()
        .map(|text| parse_bound(&text))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(2);
        })
        .unwrap_or(0);
    let end = args
        .next()
        .map(|text| parse_bound(&text))
        .transpose()
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(2);
        })
        .unwrap_or(start.saturating_add(64));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        eprintln!("{path}: {error}");
        std::process::exit(2);
    });
    let view = polytopia_sim::search::starting_view_from_json(&text).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    match polytopia_sim::search::search(&view, start..end) {
        Ok(seeds) => {
            for seed in &seeds {
                println!("{seed}");
            }
            println!("{} seed(s) in {start}..{end}", seeds.len());
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}

fn parse_bound(text: &str) -> Result<i32, String> {
    text.parse::<i32>().map_err(|_| format!("seed bound '{text}' is not an i32"))
}
