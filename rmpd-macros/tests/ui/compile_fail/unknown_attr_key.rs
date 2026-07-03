use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum UnknownAttrKey {
    #[command(name = "play", role = 4)]
    Play,
}

fn main() {}
