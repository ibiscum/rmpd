use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum Command {
    #[command(name = "play", name = "stop")]
    Play,
}

fn main() {}
