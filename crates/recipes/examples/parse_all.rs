//! Parses every FP1 file named on the command line and reports any that fail.
fn main() {
    let (mut ok, mut bad) = (0, 0);
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        match xrp_recipes::fp1::Recipe::parse(&text) {
            Ok(r) if !r.fields.is_empty() => ok += 1,
            Ok(_) => { bad += 1; println!("EMPTY {path}") }
            Err(e) => { bad += 1; println!("FAIL {path}: {e}") }
        }
    }
    println!("{ok} read, {bad} failed");
}
