//! Times running scripts, to see where the time goes:
//! `cargo run --release -p inkframe-script --example startup`.

use std::time::Instant;

use inkframe_core::Document;
use inkframe_script::{Script, run};

fn main() {
    let rig =
        Document::load(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/rig.inkf")))
            .unwrap();
    let time = |label: &str, source: &str, doc: &Document| {
        let mut runs = Vec::new();
        for _ in 0..20 {
            let started = Instant::now();
            run(Script::new(source, doc.clone())).unwrap();
            runs.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        runs.sort_by(f64::total_cmp);
        println!("{label}: {:.2} ms (median of 20)", runs[10]);
    };
    time("empty script, new project", "", &Document::default());
    time("empty script, rig", "", &rig);
    time("list layers, rig", "inkframe.document.layers.map((l) => l.name)", &rig);
    time(
        "add 50 layers",
        "for (let i = 0; i < 50; i++) inkframe.document.addLayer(`L${i}`, { rect: [10, 10] })",
        &Document::default(),
    );
}
