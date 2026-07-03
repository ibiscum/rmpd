use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum PermissionOutOfRange {
    #[command(name = "play", permission = 300)]
    Play,
}

fn main() {}
