#[cfg(not(target_arch = "wasm32"))]
fn main() -> anyhow::Result<()> {
    deckhand::cli::main()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
