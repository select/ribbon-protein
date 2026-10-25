//! Native JSON driver for parity checks: cargo run --release -p ribbon-core
//! --example render < request.json > result.json
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = String::new();
    io::stdin()
        .take(32 * 1024 * 1024 + 1)
        .read_to_string(&mut request)?;
    println!("{}", ribbon_core::render_json_native(&request)?);
    Ok(())
}
