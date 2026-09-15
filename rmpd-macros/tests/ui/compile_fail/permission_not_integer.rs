use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum PermissionNotInteger {
    #[command(name = "play", permission = "admin")]
    Play,
}

fn main() {}
