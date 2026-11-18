//! The crate's own `herdr` bin (fork: FORK.md): herdr's integration tests spawn it. Not shipped; `jocasta` is.

fn main() -> std::io::Result<()> {
    herdr::main()
}
