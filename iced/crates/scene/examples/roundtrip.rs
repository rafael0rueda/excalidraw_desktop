//! Checks that drawings survive a load and save unchanged.
//!
//!   cargo run -p scene --example roundtrip -- drawing.excalidraw…
fn main() {
    let mut failed = false;
    for path in std::env::args().skip(1) {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                println!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        match scene::Scene::parse(&text) {
            Err(e) => {
                println!("{path}: {e}");
                failed = true;
            }
            Ok(scene) => {
                let written = scene.to_json();
                let kinds = scene.elements.len();
                if written == text {
                    println!("{path}: identical ({kinds} elements)");
                } else {
                    failed = true;
                    let line = written.lines().zip(text.lines()).position(|(a, b)| a != b);
                    println!("{path}: DIFFERS at line {line:?} ({kinds} elements)");
                    if let Some(l) = line {
                        println!(
                            "  wrote: {:?}\n  read:  {:?}",
                            written.lines().nth(l),
                            text.lines().nth(l)
                        );
                    }
                }
            }
        }
    }
    std::process::exit(i32::from(failed));
}
