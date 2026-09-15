use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
enum NameNotString {
    #[command(name = 123)]
    Play,
}

fn main() {}
