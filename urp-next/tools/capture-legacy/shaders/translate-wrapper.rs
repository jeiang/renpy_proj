// Compiled only by capture.py, substituting the supplied repository's module path.
// No copy of the legacy translator is maintained here.
#[allow(dead_code)]
#[path = __TRANSLATOR_PATH__]
mod translate;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 4 {
        return Err("expected ORIGINAL_VERT ORIGINAL_FRAG TRANSLATED_VERT TRANSLATED_FRAG".into());
    }
    let vertex = std::fs::read_to_string(&args[0])?;
    let fragment = std::fs::read_to_string(&args[1])?;
    let program = translate::translate(&vertex, &fragment)
        .map_err(|error| format!("translating {:?} and {:?}: {error}", args[0], args[1]))?;
    std::fs::write(&args[2], program.vs)?;
    std::fs::write(&args[3], program.fs)?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("shader capture: {error}");
        std::process::exit(1);
    }
}
