use std::{io, sync::Arc};
fn main() -> io::Result<()> {
    console_runtime::run(
        io::stdin(),
        io::stdout(),
        Arc::new(console_runtime::Metrics::default()),
        |_| Ok(()),
    )
}
