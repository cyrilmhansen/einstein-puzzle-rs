use std::error::Error;
use std::io::{self, BufWriter, Write};

use einstein_core::export::write_generated_json;
use einstein_core::generator::generate;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let first: u64 = args.next().map_or(Ok(0), |s| s.parse())?;
    let count: u64 = args.next().map_or(Ok(1), |s| s.parse())?;
    if args.next().is_some() || count == 0 || first.checked_add(count - 1).is_none() {
        return Err(
            "usage: emit_corpus [first_seed] [count]; count must be positive without seed overflow"
                .into(),
        );
    }
    let mut out = BufWriter::new(io::stdout().lock());
    for offset in 0..count {
        let seed = first + offset;
        let puzzle = generate(seed).map_err(|e| format!("seed {seed}: {e}"))?;
        write_generated_json(&mut out, &puzzle)?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}
