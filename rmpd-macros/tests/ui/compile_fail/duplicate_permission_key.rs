use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum Command {
    #[command(name = "play", permission = 1, permission = 2)]
    Play,
}

fn main() {}
